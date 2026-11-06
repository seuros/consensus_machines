use crate::{Entry, Position};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Log<const L: usize> {
    entries: [Option<Entry>; L],
    len: usize,
}
impl<const L: usize> Log<L> {
    pub(crate) const fn new() -> Self {
        Self {
            entries: [None; L],
            len: 0,
        }
    }
    pub(crate) const fn len(&self) -> usize {
        self.len
    }
    pub(crate) const fn entry(&self, index: usize) -> Option<Entry> {
        if index == 0 || index > self.len {
            None
        } else {
            self.entries[index - 1]
        }
    }
    pub(crate) fn last(&self) -> Position {
        Position {
            index: self.len,
            term: self.entry(self.len).map_or(0, |entry| entry.term),
        }
    }
    pub(crate) const fn push(&mut self, entry: Entry) -> Option<usize> {
        if self.len == L {
            return None;
        }
        self.entries[self.len] = Some(entry);
        self.len += 1;
        Some(self.len)
    }
    pub(crate) fn append(
        &mut self,
        previous: Position,
        incoming: &[Entry],
        committed: usize,
    ) -> Option<usize> {
        if previous.index > self.len
            || self.entry(previous.index).map_or(0, |entry| entry.term) != previous.term
        {
            return None;
        }
        let end = previous.index.checked_add(incoming.len())?;
        if end > L {
            return None;
        }
        let mut replace = None;
        for (offset, entry) in incoming.iter().enumerate() {
            let index = previous.index + offset + 1;
            match self.entry(index) {
                Some(old) if old.term == entry.term => {
                    // Equal term/index with different command data is an invalid history.
                    if old.command != entry.command {
                        return None;
                    }
                }
                Some(_) if index <= committed => return None,
                Some(_) | None => {
                    replace = Some(offset);
                    break;
                }
            }
        }
        if let Some(offset) = replace {
            let keep = previous.index + offset;
            self.entries[keep..self.len].fill(None);
            self.len = keep;
            for entry in &incoming[offset..] {
                self.push(*entry)?;
            }
        }
        Some(end)
    }
}
