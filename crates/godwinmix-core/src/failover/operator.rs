//! What the operator can say about a backup that is on.

use super::Board;

impl Board {
    /// Go back to the source now. Refused when it is not on its backup.
    pub fn release(&mut self, id: &str) -> Result<(), String> {
        let entry = self.entries.get_mut(id).filter(|e| e.watch.engaged());
        let Some(entry) = entry else {
            return Err(format!("{id} is not on its backup, so there is nothing to return from."));
        };
        entry.watch.disengage();
        entry.leaving = Some("operator");
        Ok(())
    }

    /// Stay on the backup until the operator returns it.
    pub fn stay(&mut self, id: &str) -> Result<(), String> {
        match self.entries.get_mut(id).filter(|e| e.watch.engaged()) {
            Some(entry) => {
                entry.watch.stay = true;
                Ok(())
            }
            None => Err(format!("{id} is not on its backup, so there is nothing to stay on.")),
        }
    }
}
