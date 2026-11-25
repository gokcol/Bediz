//! Tutorial text bundled with the app. Keep action names aligned with the UI.
use crate::appearance::{Icon, TileTone};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Chapter {
    Capture,
    Annotate,
    Recording,
    Customization,
}

impl Chapter {
    pub(super) const ALL: [Self; 4] = [
        Self::Capture,
        Self::Annotate,
        Self::Recording,
        Self::Customization,
    ];

    pub(super) fn presentation(self) -> (&'static str, Icon, TileTone) {
        match self {
            Self::Capture => ("Capture", Icon::Capture, TileTone::Blue),
            Self::Annotate => ("Annotate", Icon::Edit, TileTone::Teal),
            Self::Recording => ("Screen Recording", Icon::Video, TileTone::Clay),
            Self::Customization => ("Customization", Icon::Settings, TileTone::Violet),
        }
    }
}

pub(super) struct Step {
    pub(super) title: &'static str,
    pub(super) detail: &'static str,
}

pub(super) struct Lesson {
    pub(super) chapter: Chapter,
    pub(super) title: &'static str,
    pub(super) overview: &'static str,
    pub(super) steps: &'static [Step],
    pub(super) tip: Option<&'static str>,
    pub(super) show_shortcut: bool,
}

const fn step(title: &'static str, detail: &'static str) -> Step {
    Step { title, detail }
}

use Chapter::{Annotate, Capture, Customization, Recording};

pub(super) const LESSONS: &[Lesson] = &[
    Lesson {
        chapter: Capture,
        title: "Take your first screenshot",
        overview: "Capture a region, an application window, or an entire display.",
        steps: &[
            step(
                "Open Capture",
                "Choose Capture image on Home. You can also click the Bediz icon in the macOS menu bar or Windows system tray to open Capture Now over your current app.",
            ),
            step(
                "Choose what to capture",
                "Use Region for a custom rectangle, Window for one application window, or Full screen for a display. Set the display first if you have more than one.",
            ),
            step(
                "Frame the shot",
                "Click Capture Now. Drag a region or click a highlighted window. In Region mode, drag the edges or corners to refine it. The magnifier helps line up the boundary with the source pixels.",
            ),
            step(
                "Confirm the capture",
                "Click Capture or press Enter. Press Esc to cancel or use the reset arrow to start the selection again. Full screen uses the whole selected display.",
            ),
        ],
        tip: Some(
            "If macOS asks for access, allow Bediz in System Settings → Privacy & Security → Screen & System Audio Recording, then reopen the app if prompted.",
        ),
        show_shortcut: true,
    },
    Lesson {
        chapter: Capture,
        title: "Send to Editor or Clipboard",
        overview: "Set where the screenshot goes and how large the finished image should be.",
        steps: &[
            step(
                "Choose a destination",
                "In Capture options, use After capture → Editor to annotate your image or Clipboard to paste it into another app. The destination icon on the selection toolbar gives you the same choice.",
            ),
            step(
                "Set the output size",
                "Keep 100% for the original pixel dimensions. Choose 50% for an image with half the width and height. You can frame the screenshot at any output size.",
            ),
            step(
                "Capture and use the result",
                "With Editor selected, the capture opens ready to annotate. With Clipboard selected, return to your other app and paste. Find saved captures in Library and recent work in the Recents strip.",
            ),
        ],
        tip: Some(
            "For menus and transient UI, set a timer before starting. Turn on Include pointer only when the pointer is part of what you want to show.",
        ),
        show_shortcut: false,
    },
    Lesson {
        chapter: Capture,
        title: "Capture several regions at once",
        overview: "Combine separate parts of the same display into one image with transparent gaps.",
        steps: &[
            step(
                "Start Multi-region capture",
                "In Capture, select Multi-region, choose a destination, and click Capture Now.",
            ),
            step(
                "Add and refine areas",
                "Drag in empty space for each area. Drag a selected region or its handles to move or resize it. To start a new overlapping region, hold Command on Mac or Ctrl on Windows as you drag.",
            ),
            step(
                "Finish the composite",
                "Select a region and press Delete or use Remove to take it out. Choose Capture or press Enter when all the regions are ready.",
            ),
        ],
        tip: Some(
            "Each region keeps its position relative to the others. Use a format that supports transparency, such as PNG, to keep the gaps clear.",
        ),
        show_shortcut: false,
    },
    Lesson {
        chapter: Capture,
        title: "Capture a scrolling page",
        overview: "Stitch a long web page, PDF, or other vertically scrolling view into one image.",
        steps: &[
            step(
                "Select a region or window",
                "In Region or Window mode, frame the content and leave the selection open. For a PDF, enable continuous scrolling in your PDF viewer.",
            ),
            step(
                "Start scrolling capture",
                "Click the Scrolling capture icon beside Capture. For a window with independently scrolling panes, scroll each pane downward in turn when the capture controller prompts you.",
            ),
            step(
                "Scroll in small steps",
                "Keep about half of the previous view visible so Bediz can line up the overlapping content. Keep the source window and its zoom steady. If matching stalls, scroll back a little and resume.",
            ),
            step(
                "Keep the result",
                "Review the preview and click Done. If Bediz only collected the initial view, choose Resume to keep scrolling or Keep visible area to save the current result.",
            ),
        ],
        tip: Some(
            "Turn off Capture immediately after selection before starting. That gives you time to choose scrolling capture from the toolbar.",
        ),
        show_shortcut: false,
    },
    Lesson {
        chapter: Capture,
        title: "Extract text from the screen",
        overview: "Copy text from a screenshot, scanned page, or an app that prevents normal text selection.",
        steps: &[
            step(
                "Frame the text",
                "Open Capture, choose Text, and click Capture Now. Drag around the lines you need. Use Copy text to start recognition.",
            ),
            step(
                "Review the result",
                "Check the recognized words and punctuation, especially small text, columns, and names. Select a portion of the result or copy it all for use in another app.",
            ),
            step(
                "Read text in an existing image",
                "In Editor, choose Select and drag a region around the text. Use Extract text in the selection controls or in the canvas context menu. With no region selected, the action reads the whole visible image.",
            ),
        ],
        tip: Some(
            "Recognition runs on your device. On Windows, recognition languages come from the installed Windows language capabilities.",
        ),
        show_shortcut: false,
    },
    Lesson {
        chapter: Capture,
        title: "Collect screenshots in an album",
        overview: "Build a walkthrough, contact sheet, or project record as you capture.",
        steps: &[
            step(
                "Create an album",
                "Open Albums → New album. Enter a name, choose a template, and review its layout preview. Click Create album.",
            ),
            step(
                "Make it available during capture",
                "In Albums, hover the row and click Accept captures. You can have up to ten albums ready, and pick which one should receive the current capture.",
            ),
            step(
                "Send the next capture there",
                "In the still-capture toolbar, open the destination menu and select the album by name. You can also use the album row’s Use as capture destination icon.",
            ),
            step(
                "Manage the collection",
                "Open the album to title or reorder entries, edit a capture, or add another image. Use Export PDF to share the album. Click Pause accepting captures on the album row when you want to stop collecting.",
            ),
        ],
        tip: Some(
            "Albums has the full collection. Albums also appear in Library and Recents alongside your projects.",
        ),
        show_shortcut: false,
    },
    Lesson {
        chapter: Annotate,
        title: "Add arrows, text, and other marks",
        overview: "Turn a screenshot into an explanation with editable artwork.",
        steps: &[
            step(
                "Open an image in Editor",
                "Capture to Editor, open an image from Home, or use File → Open Project to pick up existing work.",
            ),
            step(
                "Draw an arrow or shape",
                "Pick Arrow or Shapes in the editor toolbar. Drag from the starting point to the end. The new object is selected, so you can adjust it immediately or drag on empty space to draw another.",
            ),
            step(
                "Add a caption or callout",
                "Pick Text and click where the caption belongs, or drag a Callout for text with a pointer. Use Aa for typeface, size, bold, italic, and underline. Double-click text later to continue typing at the end.",
            ),
            step(
                "Point out a sequence",
                "Use Step to place numbered markers and Highlight to make an area stand out. Change size, line width, fill, or color with the floating controls or the Styles panel.",
            ),
        ],
        tip: Some(
            "Hover an icon to see its name. If a tool is missing from the main ribbon, look under More tools.",
        ),
        show_shortcut: false,
    },
    Lesson {
        chapter: Annotate,
        title: "Select objects or edit a region",
        overview: "Use one Select tool for object selection and pixel-area edits.",
        steps: &[
            step(
                "Click an object",
                "Click an annotation to select it. Drag the object to move it or use its handles to resize or reshape it. Shift-click adds or removes an object from the selection.",
            ),
            step(
                "Drag a pixel selection",
                "Choose Select and drag a rectangle. You can start it outside the image in the surrounding canvas area and drag inward.",
            ),
            step(
                "Choose which layers it affects",
                "Open the stack icon on the selection toolbar. Visible image uses the visible composition. Active layer targets one layer. Choose layers lets you check specific layers.",
            ),
            step(
                "Move, copy, or clear the area",
                "Drag inside the region to move it, or use the context menu to copy, cut, or delete it. Review the selected scope first when working across several layers.",
            ),
        ],
        tip: Some(
            "Paste text to create an editable text object. Paste an image while editing to place it as its own image layer.",
        ),
        show_shortcut: false,
    },
    Lesson {
        chapter: Annotate,
        title: "Arrange and group layers",
        overview: "Organize overlapping text, drawings, privacy effects, and images.",
        steps: &[
            step(
                "Open Layers",
                "Click Properties in the editor toolbar, then the Layers icon in the panel header. Click a row to select its object. Use the eye to hide it and the lock to protect it from edits.",
            ),
            step(
                "Find a covered object",
                "Right-click its location on the canvas and open Layers at this point. Choose the layer you want even when another object covers it.",
            ),
            step(
                "Change the stack order",
                "Open the selected object’s Layer order icon. Move backward or forward shifts it by one position. Send to back or Bring to front moves it to an end of the stack.",
            ),
            step(
                "Group related objects",
                "Shift-click the objects or their layer rows, then click Group in Layers. Expand the group to edit a member on its own, or select the group and choose Ungroup to separate it.",
            ),
        ],
        tip: Some(
            "Arrange can align or distribute multiple objects. Click the Layers info icon for the grouping and layer controls at a glance.",
        ),
        show_shortcut: false,
    },
    Lesson {
        chapter: Annotate,
        title: "Hide details and clean up the image",
        overview: "Use a privacy effect, recolor a background, or adjust the image itself.",
        steps: &[
            step(
                "Cover private information",
                "Choose Blur or Pixelate and drag over the part to cover. Increase the effect strength and check the result at full size. For text or numbers that must be fully hidden, cover them with an opaque shape and inspect the flattened output.",
            ),
            step(
                "Recolor a connected area",
                "Choose Fill, pick a color, then click the area. Adjust the edge controls to include similar nearby colors or get a smoother boundary.",
            ),
            step(
                "Crop or resize the composition",
                "Use Image beside Properties for Crop, Resize image, Canvas size, rotate, and flip. Crop sets the visible area. Canvas size changes its bounds. Resize image scales the output pixels.",
            ),
            step(
                "Apply a finishing effect",
                "Open Quick Effects in Properties. Apply a border, shadow, or edge style and adjust it in the panel.",
            ),
        ],
        tip: Some(
            "Share a flattened copy when the image contains private details. An editable project retains source pixels and lets someone remove its annotations and effects.",
        ),
        show_shortcut: false,
    },
    Lesson {
        chapter: Annotate,
        title: "Save an editable project and share a copy",
        overview: "Keep the original and all its annotations together, then export the result you want to share.",
        steps: &[
            step(
                "Save your work",
                "Choose File → Save Project, or press Command+S on Mac or Ctrl+S on Windows. The .myimage project keeps the original pixels, editable annotations, and image adjustments together.",
            ),
            step(
                "Export a copy",
                "Use File → Save As and choose a format. PNG preserves transparency, JPEG is useful for photos or smaller files, and PDF works for a document handoff.",
            ),
            step(
                "Open the project again",
                "Use File → Open Project or find it in Library or Recents. To print the current result, use File → Print.",
            ),
        ],
        tip: Some(
            "Use App menu → Recover unfinished work for a recovery copy after an interrupted session. Save it as a project when you are ready to keep it.",
        ),
        show_shortcut: false,
    },
    Lesson {
        chapter: Recording,
        title: "Record a window or display",
        overview: "Record an app demonstration or a full-screen walkthrough as an MP4.",
        steps: &[
            step(
                "Open video capture",
                "Choose Record video on Home or Video in Capture. Select Window to limit the recording to an app, or Full screen to record one display.",
            ),
            step(
                "Choose the source",
                "Start the capture setup. For Window, pick a preview in the window chooser and click Use window. For Full screen, confirm that the intended display is selected.",
            ),
            step(
                "Check your settings and start",
                "Use Recording options for quality, frame rate, audio, countdown, and maximum duration. Click Record when the frame and sources are ready.",
            ),
            step(
                "Pause, resume, or save",
                "Use the recording controls to pause and resume the same recording. Stop and save opens the finished recording in Bediz. The source MP4 is saved in the Recordings folder under your storage location.",
            ),
        ],
        tip: Some(
            "Close or move sensitive windows and silence notifications before capturing a full display.",
        ),
        show_shortcut: false,
    },
    Lesson {
        chapter: Recording,
        title: "Record two windows together",
        overview: "Show two apps in one recording and adjust how each is framed.",
        steps: &[
            step(
                "Choose Two windows",
                "In Video capture, select Two windows and start the setup. Open the window picker and choose two window previews. Use the title filter if the list is long.",
            ),
            step(
                "Arrange the composite",
                "Click Arrange windows. Choose a layout, preview both windows, and adjust the options for each window in the arrangement panel.",
            ),
            step(
                "Review and record",
                "Check that both previews show the content you intend to share. Configure the recording’s audio and quality, then click Record. Stop and save when finished.",
            ),
        ],
        tip: Some("Keep both source windows open throughout the recording."),
        show_shortcut: false,
    },
    Lesson {
        chapter: Recording,
        title: "Set quality, frame rate, and audio",
        overview: "Choose settings that fit the content, expected recording length, and available storage.",
        steps: &[
            step(
                "Set picture quality and frame rate",
                "Open Recording options. Choose a quality preset and set the frame rate. 30 fps suits most walkthroughs. Pick a higher preset or enter a custom rate for motion. Bediz can fall back to 30 fps when the requested rate is unavailable.",
            ),
            step(
                "Pick an audio source",
                "Choose Off, System audio for computer sound, or Microphone for your voice. With audio on, use the quality controls to adjust the bitrate and sample rate. On Mac, Microphone mode also offers noise reduction.",
            ),
            step(
                "Set a countdown and time limit",
                "Use the countdown to give yourself time to prepare before the recording starts. Set Maximum recording time to cap the length.",
            ),
            step(
                "Check expected size",
                "Choose a recording target to see the estimated MP4 size and free space. If space is tight, shorten the duration, use a lower quality, or choose a different storage folder.",
            ),
        ],
        tip: Some(
            "Make a short test recording when changing microphone or audio settings. Play it back in Bediz and check the sound before a longer session.",
        ),
        show_shortcut: false,
    },
    #[cfg(target_os = "macos")]
    Lesson {
        chapter: Recording,
        title: "Add a presenter camera",
        overview: "Preview your camera and place it over the screen recording.",
        steps: &[
            step(
                "Open Presenter camera",
                "During video setup, click the camera icon. Turn on Include presenter camera and allow Camera access if macOS asks.",
            ),
            step(
                "Frame your camera",
                "Check Live preview. Set the size and choose a corner, or choose free position and drag the bubble inside the target. Turn on Mirror camera if you prefer the mirrored view.",
            ),
            step(
                "Verify the result",
                "Start a short recording, then play it in Bediz. Check that the bubble stays clear of menus or other important content.",
            ),
        ],
        tip: Some(
            "You can turn off the presenter camera in the setup panel before starting another recording.",
        ),
        show_shortcut: false,
    },
    Lesson {
        chapter: Recording,
        title: "Open a video and annotate a frame",
        overview: "Add Bediz arrows, text, shapes, and other editable marks to a timed section of a video.",
        steps: &[
            step(
                "Open the footage",
                "Use Home → Open video and select an MP4 or MOV, or pick a recording from Library or Recents. Play it in Bediz, then pause and position the playhead at the moment you want to mark up.",
            ),
            step(
                "Annotate the reference frame",
                "Click Annotate This Frame. Bediz opens a locked reference frame in the Editor. Add and position your artwork over it with the regular annotation tools.",
            ),
            step(
                "Set how long the annotations appear",
                "Set Duration, then click Send to Video. The overlay starts at the frame’s time and stays visible for the duration you set.",
            ),
            step(
                "Reopen and revise it",
                "Select the overlay in the timeline’s annotation list and choose Edit annotation. Change the original objects in the frame editor. Send to Video updates the same overlay and keeps its timing unless you change the duration.",
            ),
        ],
        tip: Some(
            "Save a Video Project to keep the overlays editable. Keep the source video files with the project.",
        ),
        show_shortcut: false,
    },
    #[cfg(target_os = "macos")]
    Lesson {
        chapter: Recording,
        title: "Cut clips and add title pages",
        overview: "Use the timeline to remove pauses, rearrange sections, and insert title or information pages.",
        steps: &[
            step(
                "Position the playhead precisely",
                "Scrub in the player or type a time into Playhead. Use Back 1 ms and Forward 1 ms for fine changes to the time. The image displayed is the video frame at that time.",
            ),
            step(
                "Cut a range or split a clip",
                "Mark the range start and end, then choose Cut, Copy, or Delete range. To move or repeat a range, move the playhead and use Paste range at playhead. Use Split at playhead to make two clips.",
            ),
            step(
                "Insert a designed page or a freeze frame",
                "Use Add page to place a page at the beginning, at the playhead, after a clip, or at the end. Pick a design, enter text or add an image, then set its duration. Use Freeze frame to hold the current video image for a set time.",
            ),
            step(
                "Finish the timing",
                "Select a clip or page and open Edit clip or page to adjust its transition. For clips with audio, you can also set fade-in and fade-out times. Use Add annotation and choose Spotlight to direct attention, then preview the result.",
            ),
        ],
        tip: Some(
            "Use short title and end pages, then give an information page enough time to read each line before it advances.",
        ),
        show_shortcut: false,
    },
    #[cfg(target_os = "windows")]
    Lesson {
        chapter: Recording,
        title: "Trim and split clips",
        overview: "Use the Windows timeline to trim a recording or remove an unwanted section.",
        steps: &[
            step(
                "Find the cut point",
                "Pause the video and position the playhead. Select the clip that contains the point you want to edit.",
            ),
            step(
                "Trim or split",
                "Use Trim start to playhead or Trim end to playhead to shorten that clip. Use Split at an interior point to make two clips.",
            ),
            step(
                "Remove an unwanted section",
                "Split on both sides of the section, select its clip, and click Remove selected clip. Use Preview Edits to check the cuts and timed annotations.",
            ),
        ],
        tip: Some("Use Show Source to return from the rendered preview to the editable timeline."),
        show_shortcut: false,
    },
    #[cfg(target_os = "macos")]
    Lesson {
        chapter: Recording,
        title: "Create subtitles or copy a transcript",
        overview: "Use macOS 26 or later to turn speech into timed text on your device.",
        steps: &[
            step(
                "Open Auto Subtitles",
                "With a video in the timeline, click Auto Subtitles. English is the default. Choose another supported language and download its speech model when prompted.",
            ),
            step(
                "Generate and review the text",
                "Start recognition and check the resulting captions against the recording. Correct missed words and timing before using the captions.",
            ),
            step(
                "Use the result",
                "Choose Add to Timeline to place the captions in the edit. Copy the transcript for notes or save an SRT file for use in other players. When exporting the movie, choose whether to include the subtitles.",
            ),
        ],
        tip: Some("Clear speech and a quiet recording make captions easier to review."),
        show_shortcut: false,
    },
    Lesson {
        chapter: Recording,
        title: "Save and export a video",
        overview: "Save the editing project first, then render a video for playback or sharing.",
        steps: &[
            step(
                "Keep the editable project",
                "Use Save Video Project to save the timeline and frame annotations in a .bedizvideo file. Keep the original recordings or imported clips available so the project can reopen them.",
            ),
            #[cfg(target_os = "macos")]
            step(
                "Choose export settings",
                "Click Export video. Choose a size preset or enter custom dimensions, then set the available video and audio options. Select subtitle output if your project has captions.",
            ),
            #[cfg(target_os = "windows")]
            step(
                "Review and export",
                "Click Preview Edits and play the rendered preview. Choose Export MP4 and select a save location for the edited video.",
            ),
            step(
                "Check the final file",
                "Play the result end to end. Check audio volume, annotations, and the transitions or cuts before sending the file.",
            ),
        ],
        tip: Some(
            "Use the project when you expect to revise the video. Use the exported file when someone needs to play the finished result.",
        ),
        show_shortcut: false,
    },
    Lesson {
        chapter: Customization,
        title: "Adjust the look and interface size",
        overview: "Choose an appearance and a control size that are comfortable on your display.",
        steps: &[
            step(
                "Open Settings",
                "Click the calico icon at the bottom right and choose Settings. Use Interface for the main theme, Capture Now for the capture panel’s look, and Icons to choose a toolbar icon style.",
            ),
            step(
                "Set the pointer and finish",
                "Choose the system pointer or the Calico pointer style, then click Done. The change is saved for the next launch.",
            ),
            step(
                "Scale the interface",
                "In the same calico App menu, use Interface size. Start with Medium and try the neighboring sizes. This changes the app’s controls and labels. Use the canvas zoom to enlarge the image on screen.",
            ),
        ],
        tip: None,
        show_shortcut: false,
    },
    Lesson {
        chapter: Customization,
        title: "Customize the toolbar and Properties panel",
        overview: "Keep frequently used tools within reach and place the inspector where you work best.",
        steps: &[
            step(
                "Choose toolbar tools",
                "With an image open in Editor, click More tools and choose Customize toolbar. Check a tool to show it in the main toolbar. Unchecked tools remain in More tools.",
            ),
            step(
                "Change the order",
                "Use the up and down arrows beside a tool to move it earlier or later. Close the panel to return to the Editor.",
            ),
            step(
                "Set up Properties",
                "Use Properties in the editor toolbar to show or hide the panel. Its header icons switch between Styles, Layers, Presets, and Quick Effects. Unpin it and drag the header grip to float it, or pin it to dock it on the right.",
            ),
        ],
        tip: Some(
            "Hide the Recents strip using its toggle when you want more canvas space. Turn it back on to switch between recent projects.",
        ),
        show_shortcut: false,
    },
    Lesson {
        chapter: Customization,
        title: "Make reusable annotation presets",
        overview: "Save a favorite callout, label, or set of marks and insert editable copies in other projects.",
        steps: &[
            step(
                "Prepare your artwork",
                "Make the annotation or group you want to reuse. Set its colors, text, and size, then select it on the canvas or in Layers.",
            ),
            step(
                "Save the preset",
                "Open Properties → Presets, choose Save selection as preset, and give it a recognizable name.",
            ),
            step(
                "Use it in another project",
                "Open the destination image. In Presets, double-click the saved item or select it and click Insert preset. Move or edit the inserted copy as needed.",
            ),
        ],
        tip: Some(
            "Quick styles apply a built-in look to an object. Presets insert a saved annotation or group with its contents and appearance.",
        ),
        show_shortcut: false,
    },
    Lesson {
        chapter: Customization,
        title: "Change the hotkey and stay ready for capture",
        overview: "Choose a comfortable shortcut and keep capture available while you work in other apps.",
        steps: &[
            step(
                "Edit the shortcut",
                "Open Capture or the Capture Now panel, then click the shortcut shown beside or below Capture Now. Choose the modifiers and key you want, then save.",
            ),
            step(
                "Try it in another app",
                "Switch to the app you want to capture and press the shortcut. Esc cancels an open selection. If Bediz is finishing a file operation, it queues the capture. If the shortcut is already taken, choose another combination or start from the menu bar or tray icon.",
            ),
            step(
                "Keep Bediz available",
                "Use Keep ready in background on Capture or Background in the calico App menu. To have Bediz ready after signing in, open Settings and enable Start Bediz at login.",
            ),
        ],
        tip: Some(
            "Quick start & shortcuts in the calico App menu shows the common editor shortcuts.",
        ),
        show_shortcut: true,
    },
    Lesson {
        chapter: Customization,
        title: "Manage storage and find your work",
        overview: "Choose where Bediz saves new work and reopen it from the workspace that suits the task.",
        steps: &[
            step(
                "Check the current folder",
                "Open the calico App menu → Storage folder or Library → Storage. Use the reveal action to see the current location in Finder or File Explorer.",
            ),
            step(
                "Choose a new storage folder",
                "Select a writable location. Review Move existing captures and projects before confirming. Leave it enabled to move the managed captures and projects, or turn it off to use the new folder for future files.",
            ),
            step(
                "Find a project again",
                "Use Library to browse and search saved work. Use Recents to jump back to something you just opened. Use Albums to manage collections and whether each accepts new captures.",
            ),
        ],
        tip: Some(
            "Recordings are kept in the Recordings subfolder. Keep referenced source videos available when moving or backing up a .bedizvideo project.",
        ),
        show_shortcut: false,
    },
];
