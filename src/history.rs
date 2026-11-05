use steam_workshop_creator::Settings;

#[derive(Default)]
pub struct History {
    undo: Vec<Settings>,
    redo: Vec<Settings>,
    grouped: bool,
}
impl History {
    pub fn finish(&mut self) {
        self.grouped = false;
    }
    pub fn record(&mut self, before: &Settings, after: &Settings, active: bool) {
        if self.grouped && !active && self.undo.last() == Some(after) {
            self.undo.pop();
            self.grouped = false;
            return;
        }
        if before != after {
            if !self.grouped {
                self.undo.push(before.clone());
                if self.undo.len() > 100 {
                    self.undo.remove(0);
                }
            }
            self.redo.clear();
            self.grouped = active;
        }
        if !active {
            self.grouped = false;
        }
    }
    pub fn undo(&mut self, current: &mut Settings) {
        if let Some(previous) = self.undo.pop() {
            self.redo.push(std::mem::replace(current, previous));
        }
        self.finish();
    }
    pub fn redo(&mut self, current: &mut Settings) {
        if let Some(next) = self.redo.pop() {
            self.undo.push(std::mem::replace(current, next));
        }
        self.finish();
    }
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn gestures_are_one_step_and_new_edits_discard_redo() {
        let mut history = History::default();
        let first = Settings::default();
        let mut second = first.clone();
        second.transform.x = 10.;
        history.record(&first, &second, true);
        let mut third = second.clone();
        third.transform.x = 20.;
        history.record(&second, &third, true);
        history.record(&third, &third, false);
        history.undo(&mut third);
        assert_eq!(third, first);
        history.redo(&mut third);
        assert_eq!(third.transform.x, 20.);
        history.undo(&mut third);
        let mut new = third.clone();
        new.monochrome = true;
        history.record(&third, &new, false);
        assert!(!history.can_redo());
    }
}
