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
    /// The first `len` slots are the log; the rest hold the placeholder
    /// `Entry::default()` and are never read.
    entries: [Entry; L],
    len: usize,
}

impl<const L: usize> Log<L> {
    pub(crate) const fn new() -> Self {
        Self {
            entries: [Entry {
                term: 0,
                command: 0,
            }; L],
            len: 0,
        }
    }

    pub(crate) const fn len(&self) -> usize {
        self.len
    }

    pub(crate) const fn entry(&self, index: usize) -> Option<Entry> {
        let Some(at) = index.checked_sub(1) else {
            return None;
        };
        if let Some((_, [entry, ..])) = self.held().split_at_checked(at) {
            Some(*entry)
        } else {
            None
        }
    }

    /// The entries held, in index order: index `i` is `held()[i - 1]`.
    pub(crate) const fn held(&self) -> &[Entry] {
        if let Some((held, _)) = self.entries.split_at_checked(self.len) {
            held
        } else {
            debug_assert!(false, "the log never holds more than its capacity");
            &[]
        }
    }

    pub(crate) fn last(&self) -> Position {
        Position {
            index: self.len,
            term: self.entry(self.len).map_or(0, |entry| entry.term),
        }
    }

    pub(crate) const fn push(&mut self, entry: Entry) -> Option<usize> {
        // The first slot past the log, or none when the log is full.
        let Some((_, [slot, ..])) = self.entries.split_at_mut_checked(self.len) else {
            return None;
        };
        *slot = entry;
        // A slot was free below `L`, so the length stays within `L`.
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
            // `previous.index` is within the log and the caller checked the
            // entries end within `L`, so this never overflows.
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

    /// Carry out a `Plan::Matched { rewrite: Some(offset) }` from [`Self::plan`]:
    /// the incoming entries from `offset` on replace the log from `previous +
    /// offset`, and any older entries past them are cleared.
    ///
    /// `None`, with nothing written, when the entries would not end within
    /// the capacity: the caller checked they do.
    pub(crate) fn rewrite(
        &mut self,
        previous: usize,
        incoming: &[Entry],
        offset: usize,
    ) -> Option<()> {
        let keep = previous.checked_add(offset)?;
        let end = previous.checked_add(incoming.len())?;
        let fresh = incoming.get(offset..)?;
        let (written, past) = self
            .entries
            .get_mut(keep..)?
            .split_at_mut_checked(fresh.len())?;
        written.copy_from_slice(fresh);
        past.iter_mut()
            .take(self.len.saturating_sub(end))
            .for_each(|stale| *stale = Entry::default());
        self.len = end;
        Some(())
    }

    fn last_term(&self, index: usize) -> u64 {
        self.entry(index).map_or(0, |entry| entry.term)
    }
}
