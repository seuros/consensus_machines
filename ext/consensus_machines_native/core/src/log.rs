use crate::{Entry, Position, Refusal};

/// What an append would do to the log, decided before anything is written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Plan {
    /// No entry at `previous` with that term.
    Conflict,
    /// `previous` matched. `rewrite` is the offset into the incoming entries
    /// from which they replace or extend the log; `None` when all are held.
    Matched { rewrite: Option<usize> },
}

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

    /// Decide an append without writing. The caller has checked that the
    /// entries end within the capacity.
    pub(crate) fn plan(
        &self,
        previous: Position,
        incoming: &[Entry],
        committed: usize,
    ) -> Result<Plan, Refusal> {
        if previous.index > self.len || self.last_term(previous.index) != previous.term {
            return Ok(Plan::Conflict);
        }
        for (offset, entry) in incoming.iter().enumerate() {
            let index = previous.index + offset + 1;
            match self.entry(index) {
                Some(held) if held.term == entry.term => {
                    if held.command != entry.command {
                        return Err(Refusal::Mismatch);
                    }
                }
                Some(_) if index <= committed => return Err(Refusal::CommittedConflict),
                Some(_) | None => {
                    return Ok(Plan::Matched {
                        rewrite: Some(offset),
                    });
                }
            }
        }
        Ok(Plan::Matched { rewrite: None })
    }

    /// Carry out a `Plan::Matched { rewrite: Some(offset) }` from [`Self::plan`].
    pub(crate) fn rewrite(&mut self, previous: usize, incoming: &[Entry], offset: usize) {
        let keep = previous + offset;
        let end = previous + incoming.len();
        self.entries[keep..self.len.max(keep)].fill(None);
        for (slot, entry) in self.entries[keep..end].iter_mut().zip(&incoming[offset..]) {
            *slot = Some(*entry);
        }
        self.len = end;
    }

    fn last_term(&self, index: usize) -> u64 {
        self.entry(index).map_or(0, |entry| entry.term)
    }
}
