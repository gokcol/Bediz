use crate::capture::DisplayInfo;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use windows::{
    Foundation::TypedEventHandler,
    Graphics::{
        Capture::{Direct3D11CaptureFramePool, GraphicsCaptureItem, GraphicsCaptureSession},
        DirectX::{Direct3D11::IDirect3DDevice, DirectXPixelFormat},
        SizeInt32,
    },
    Win32::{
        Foundation::{E_FAIL, E_POINTER, HMODULE, HWND, POINT},
        Graphics::{
            Direct3D::{D3D_DRIVER_TYPE_HARDWARE, D3D_DRIVER_TYPE_WARP},
            Direct3D11::{
                D3D11_CPU_ACCESS_READ, D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_MAP_READ,
                D3D11_MAPPED_SUBRESOURCE, D3D11_SDK_VERSION, D3D11_TEXTURE2D_DESC,
                D3D11_USAGE_STAGING, D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext,
                ID3D11Texture2D,
            },
            Dxgi::IDXGIDevice,
            Gdi::{MONITOR_DEFAULTTONULL, MonitorFromPoint},
        },
        System::WinRT::{
            Direct3D11::{CreateDirect3D11DeviceFromDXGIDevice, IDirect3DDxgiInterfaceAccess},
            Graphics::Capture::IGraphicsCaptureItemInterop,
        },
        UI::WindowsAndMessaging::{GetWindowThreadProcessId, IsWindow},
    },
    core::{Error, IInspectable, Interface, Result, factory},
};

const FORMAT: DirectXPixelFormat = DirectXPixelFormat::B8G8R8A8UIntNormalized;
const BUFFERS: i32 = 2;

pub(super) enum Target {
    Display { id: u32, point: POINT },
    Window(u32),
}

impl Target {
    pub(super) fn new(display: &DisplayInfo, window: Option<u32>) -> Self {
        if let Some(window) = window {
            Self::Window(window)
        } else {
            Self::Display {
                id: display.id,
                point: POINT {
                    x: display.x.saturating_add((display.width / 2) as i32),
                    y: display.y.saturating_add((display.height / 2) as i32),
                },
            }
        }
    }

    fn item(self) -> Result<GraphicsCaptureItem> {
        let factory = factory::<GraphicsCaptureItem, IGraphicsCaptureItemInterop>()?;
        // SAFETY: Validate the handle against the selected window or monitor before opening it.
        unsafe {
            match self {
                Self::Window(id) => {
                    let window = HWND(id as usize as *mut _);
                    let mut process = 0;
                    if !IsWindow(Some(window)).as_bool()
                        || GetWindowThreadProcessId(window, Some(&mut process)) == 0
                        || process == std::process::id()
                    {
                        return Err(Error::new(
                            E_FAIL,
                            "The selected window is no longer available.",
                        ));
                    }
                    factory.CreateForWindow(window)
                }
                Self::Display { id, point } => {
                    let monitor = MonitorFromPoint(point, MONITOR_DEFAULTTONULL);
                    if monitor.0.is_null() || monitor.0 as u32 != id {
                        return Err(Error::new(
                            E_FAIL,
                            "The selected display changed. Choose it again.",
                        ));
                    }
                    factory.CreateForMonitor(monitor)
                }
            }
        }
    }
}

pub(super) struct Frames {
    item: GraphicsCaptureItem,
    closed_token: i64,
    session: GraphicsCaptureSession,
    pool: Direct3D11CaptureFramePool,
    arrived: Arc<AtomicBool>,
    closed: Arc<AtomicBool>,
    device: IDirect3DDevice,
    d3d: ID3D11Device,
    context: ID3D11DeviceContext,
    staging: Option<ID3D11Texture2D>,
    capture_size: SizeInt32,
    mapping: Option<FrameMap>,
}

impl Frames {
    pub(super) fn new(target: Target, cursor: bool) -> Result<Self> {
        if !GraphicsCaptureSession::IsSupported()? {
            return Err(Error::new(
                E_FAIL,
                "Windows Graphics Capture is unavailable on this device.",
            ));
        }
        let item = target.item()?;
        let capture_size = item.Size()?;
        let mut d3d = None;
        let mut context = None;
        // SAFETY: Both optional COM output pointers refer to live local storage on this worker.
        unsafe {
            D3D11CreateDevice(
                None,
                D3D_DRIVER_TYPE_HARDWARE,
                HMODULE::default(),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                None,
                D3D11_SDK_VERSION,
                Some(&mut d3d),
                None,
                Some(&mut context),
            )
            .or_else(|_| {
                D3D11CreateDevice(
                    None,
                    D3D_DRIVER_TYPE_WARP,
                    HMODULE::default(),
                    D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                    None,
                    D3D11_SDK_VERSION,
                    Some(&mut d3d),
                    None,
                    Some(&mut context),
                )
            })?;
        }
        let d3d = d3d.ok_or_else(|| Error::from_hresult(E_FAIL))?;
        let context = context.ok_or_else(|| Error::from_hresult(E_FAIL))?;
        let dxgi: IDXGIDevice = d3d.cast()?;
        // SAFETY: The DXGI device was constructed on the recording worker and remains live.
        let device = unsafe { CreateDirect3D11DeviceFromDXGIDevice(&dxgi) }?.cast()?;
        let pool =
            Direct3D11CaptureFramePool::CreateFreeThreaded(&device, FORMAT, BUFFERS, capture_size)?;
        let arrived = Arc::new(AtomicBool::new(false));
        let closed = Arc::new(AtomicBool::new(false));
        let frames_pending = Arc::clone(&arrived);
        pool.FrameArrived(
            &TypedEventHandler::<Direct3D11CaptureFramePool, IInspectable>::new(move |_, _| {
                frames_pending.store(true, Ordering::Release);
                Ok(())
            }),
        )?;
        let session = pool.CreateCaptureSession(&item)?;
        session.SetIsCursorCaptureEnabled(cursor)?;
        let target_closed = Arc::clone(&closed);
        let closed_token = item.Closed(
            &TypedEventHandler::<GraphicsCaptureItem, IInspectable>::new(move |_, _| {
                target_closed.store(true, Ordering::Release);
                Ok(())
            }),
        )?;
        let frames = Self {
            item,
            closed_token,
            session,
            pool,
            arrived,
            closed,
            device,
            d3d,
            context,
            staging: None,
            capture_size,
            mapping: None,
        };
        frames.session.StartCapture()?;
        Ok(frames)
    }

    pub(super) fn closed(&self) -> bool {
        self.closed.load(Ordering::Acquire)
    }

    pub(super) fn poll(&mut self, width: u32, height: u32, pixels: &mut [u8]) -> Result<bool> {
        if !self.arrived.swap(false, Ordering::AcqRel) {
            return Ok(false);
        }
        // Drain at most the pool capacity and use the newest frame.
        let mut latest = None;
        for _ in 0..BUFFERS {
            match self.pool.TryGetNextFrame() {
                Ok(frame) => {
                    if let Some(previous) = latest.replace(frame) {
                        let _ = previous.Close();
                    }
                }
                Err(error)
                    if latest.is_some() || error.code().is_ok() || error.code() == E_POINTER =>
                {
                    break;
                }
                Err(error) => return Err(error),
            }
        }
        let Some(frame) = latest else {
            return Ok(false);
        };
        let size = frame.ContentSize()?;
        if size.Width <= 0 || size.Height <= 0 {
            frame.Close()?;
            return Ok(false);
        }
        if size != self.capture_size {
            frame.Close()?;
            self.pool.Recreate(&self.device, FORMAT, BUFFERS, size)?;
            self.capture_size = size;
            self.staging = None;
            self.mapping = None;
            return Ok(false);
        }
        let access: IDirect3DDxgiInterfaceAccess = frame.Surface()?.cast()?;
        // SAFETY: The source texture belongs to this live frame. Map and unmap
        // the staging texture on the same worker that owns the device context.
        unsafe {
            let source: ID3D11Texture2D = access.GetInterface()?;
            if self.staging.is_none() {
                let mut desc = D3D11_TEXTURE2D_DESC::default();
                source.GetDesc(&mut desc);
                if desc.Width < size.Width as u32 || desc.Height < size.Height as u32 {
                    frame.Close()?;
                    return Ok(false);
                }
                desc.Usage = D3D11_USAGE_STAGING;
                desc.CPUAccessFlags = D3D11_CPU_ACCESS_READ.0 as u32;
                desc.BindFlags = 0;
                desc.MiscFlags = 0;
                self.d3d
                    .CreateTexture2D(&desc, None, Some(&mut self.staging))?;
                self.mapping = Some(FrameMap::new(
                    size.Width as u32,
                    size.Height as u32,
                    width,
                    height,
                ));
            }
            let staging = self
                .staging
                .as_ref()
                .ok_or_else(|| Error::from_hresult(E_FAIL))?;
            self.context.CopyResource(staging, &source);
            let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
            self.context
                .Map(staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))?;
            let pitch = mapped.RowPitch as usize;
            if pitch < size.Width as usize * 4 || mapped.pData.is_null() {
                self.context.Unmap(staging, 0);
                return Err(Error::new(
                    E_FAIL,
                    "Unsupported screen capture pixel layout.",
                ));
            }
            let data =
                std::slice::from_raw_parts(mapped.pData.cast::<u8>(), pitch * size.Height as usize);
            self.mapping.as_ref().unwrap().nv12(data, pitch, pixels);
            self.context.Unmap(staging, 0);
        }
        frame.Close()?;
        Ok(true)
    }
}

impl Drop for Frames {
    fn drop(&mut self) {
        let _ = self.session.Close();
        let _ = self.pool.Close();
        let _ = self.item.RemoveClosed(self.closed_token);
    }
}

struct FrameMap {
    width: usize,
    height: usize,
    offset: [usize; 2],
    xs: Vec<usize>,
    ys: Vec<usize>,
}

impl FrameMap {
    fn new(source_width: u32, source_height: u32, width: u32, height: u32) -> Self {
        let scale = (f64::from(width) / f64::from(source_width))
            .min(f64::from(height) / f64::from(source_height));
        let w = ((f64::from(source_width) * scale).round() as usize & !1).max(2);
        let h = ((f64::from(source_height) * scale).round() as usize & !1).max(2);
        let (width, height) = (width as usize, height as usize);
        Self {
            width,
            height,
            offset: [((width - w) / 2) & !1, ((height - h) / 2) & !1],
            xs: (0..w)
                .map(|x| (x * source_width as usize / w) * 4)
                .collect(),
            ys: (0..h).map(|y| y * source_height as usize / h).collect(),
        }
    }

    fn nv12(&self, bgra: &[u8], pitch: usize, output: &mut [u8]) {
        let (luma, chroma) = output.split_at_mut(self.width * self.height);
        luma.fill(16);
        chroma.fill(128);
        let [left, top] = self.offset;
        for (row, source_rows) in self.ys.chunks_exact(2).enumerate() {
            let y = top + row * 2;
            let mut uv = y / 2 * self.width + left;
            let mut y1 = y * self.width + left;
            let mut y2 = y1 + self.width;
            let (first, second) = (source_rows[0] * pitch, source_rows[1] * pitch);
            for columns in self.xs.chunks_exact(2) {
                let a = rgb(bgra, first + columns[0]);
                let b = rgb(bgra, first + columns[1]);
                let c = rgb(bgra, second + columns[0]);
                let d = rgb(bgra, second + columns[1]);
                luma[y1] = y_value(a);
                luma[y1 + 1] = y_value(b);
                luma[y2] = y_value(c);
                luma[y2 + 1] = y_value(d);
                let r = a[0] + b[0] + c[0] + d[0];
                let g = a[1] + b[1] + c[1] + d[1];
                let b = a[2] + b[2] + c[2] + d[2];
                // Limited-range BT.709, averaged over the NV12 2 × 2 chroma block.
                chroma[uv] = (((-26 * r - 87 * g + 112 * b + 512) >> 10) + 128) as u8;
                chroma[uv + 1] = (((112 * r - 102 * g - 10 * b + 512) >> 10) + 128) as u8;
                y1 += 2;
                y2 += 2;
                uv += 2;
            }
        }
    }
}

fn rgb(bgra: &[u8], offset: usize) -> [i32; 3] {
    let bgr = &bgra[offset..][..3];
    [i32::from(bgr[2]), i32::from(bgr[1]), i32::from(bgr[0])]
}

fn y_value([r, g, b]: [i32; 3]) -> u8 {
    (((47 * r + 157 * g + 16 * b + 128) >> 8) + 16) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn screen_pixels_convert_to_nv12_without_using_row_padding() {
        let mapping = FrameMap::new(2, 2, 4, 2);
        let data = [
            0, 0, 255, 255, 0, 0, 255, 255, 255, 255, 255, 255, 0, 0, 255, 255, 0, 0, 255, 255,
            255, 255, 255, 255,
        ];
        let mut nv12 = [0; 12];
        mapping.nv12(&data, 12, &mut nv12);
        assert_eq!(nv12, [63, 63, 16, 16, 63, 63, 16, 16, 102, 240, 128, 128]);
    }
}
