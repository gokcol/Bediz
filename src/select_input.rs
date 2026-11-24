use super::*;

#[derive(Clone, Copy)]
pub(super) struct SelectPress {
    screen: Pos2,
    at: Point,
    modifiers: Modifiers,
}

impl EditorState {
    pub(super) fn selected_handle(
        &self,
        at: Point,
        scale: f32,
        document: &Document,
    ) -> Option<Handle> {
        let selected = self.selected_indices(document);
        if !self.can_edit_selection(document)
            || selected.iter().any(|&i| !document.annotations[i].visible())
        {
            return None;
        }
        if selected.len() > 1 {
            selection_handle(document, &selected, at, scale)
        } else {
            self.selected
                .and_then(|i| document.annotations.get(i))
                .and_then(|a| hit_document_handle(a, at, scale, document))
        }
    }

    pub(super) fn press_drawing_object(
        &mut self,
        screen: Pos2,
        at: Point,
        scale: f32,
        modifiers: Modifiers,
        document: &Document,
    ) -> bool {
        if !self.draws_objects()
            || modifiers.alt
            || (hit_document_annotation(document, at, scale).is_none()
                && self.selected_handle(at, scale, document).is_none())
        {
            return false;
        }
        self.select_press = Some(SelectPress {
            screen,
            at,
            modifiers,
        });
        true
    }

    pub(super) fn press_select(
        &mut self,
        screen: Pos2,
        at: Point,
        scale: f32,
        modifiers: Modifiers,
        document: &mut Document,
    ) {
        if self.tool == Tool::Area
            && (self.over_area_handle(at, document) || self.inside_area(at, document))
        {
            self.begin_area_drag(at, document);
            return;
        }
        if self.tool == Tool::Select && !modifiers.shift && !modifiers.alt {
            let selected = self.selected_indices(document);
            let on_selected =
                hit_document_annotation(document, at, scale).is_some_and(|i| selected.contains(&i));
            if self.selected_handle(at, scale, document).is_some()
                || (on_selected && self.can_edit_selection(document))
            {
                self.begin_selection_gesture(at, scale, document, false, false);
                return;
            }
        }
        self.select_press = Some(SelectPress {
            screen,
            at,
            modifiers,
        });
    }

    pub(super) fn settle_select_press(
        &mut self,
        ui: &egui::Ui,
        scale: f32,
        document: &mut Document,
    ) -> bool {
        let Some(press) = self.select_press else {
            return false;
        };
        for (screen, released) in ui.input(|i| {
            let first_motion = i.events.iter().position(|event| matches!(
                event,
                egui::Event::PointerButton {pos, button: PointerButton::Primary, pressed: true, ..}
                    if *pos == press.screen
            )).map_or(0, |index| index + 1);
            i.events[first_motion..]
                .iter()
                .filter_map(|event| match event {
                    egui::Event::PointerMoved(p) => Some((*p, false)),
                    egui::Event::PointerButton {
                        pos,
                        button: PointerButton::Primary,
                        pressed: false,
                        ..
                    } => Some((*pos, true)),
                    _ => None,
                })
                .collect::<Vec<_>>()
        }) {
            if self.resolve_select_motion(screen, released, scale, document) {
                return true;
            }
        }
        false
    }

    pub(super) fn resolve_select_motion(
        &mut self,
        screen: Pos2,
        released: bool,
        scale: f32,
        document: &mut Document,
    ) -> bool {
        let Some(press) = self.select_press else {
            return false;
        };
        let dragged = screen.distance(press.screen) > 3.;
        if !dragged && !released {
            return false;
        }
        let hit = hit_document_annotation(document, press.at, scale);
        if self.draws_objects() {
            let handle = self.selected_handle(press.at, scale, document);
            let on_selected =
                hit.is_some_and(|index| self.selected_indices(document).contains(&index));
            let can_transform = !press.modifiers.shift
                && (handle.is_some() || (on_selected && self.can_edit_selection(document)));
            if dragged && self.tool.places_on_click() && !can_transform && !released {
                return false;
            }
            self.select_press = None;
            if dragged {
                if can_transform {
                    self.begin_selection_gesture(press.at, scale, document, false, false);
                } else if !self.tool.places_on_click() {
                    self.begin_tool_gesture(press.at, document);
                }
            } else if handle.is_none() {
                self.choose_annotation(hit, press.modifiers.shift, false, document);
            }
        } else if dragged {
            self.select_press = None;
            if press.modifiers.shift && hit.is_none() {
                self.area = None;
                self.area_mask.clear();
                self.pixel_mask = None;
                self.tool = Tool::Select;
                self.begin_selection_gesture(press.at, scale, document, true, false);
            } else {
                let active = self.selected;
                if self.tool == Tool::Select {
                    self.clear_selection();
                }
                self.tool = Tool::Area;
                self.region_scope = region_scope::RegionScope::for_region(active);
                self.begin_area_drag(press.at, document);
            }
        } else {
            self.select_press = None;
            if self.tool == Tool::Area || !press.modifiers.shift {
                self.deselect_pixels();
            }
            self.tool = Tool::Select;
            self.choose_annotation(hit, press.modifiers.shift, press.modifiers.alt, document);
        }
        true
    }
}
