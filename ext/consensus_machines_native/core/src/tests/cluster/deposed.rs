//! Deposed leaders that stay alive. A leader cut off from the cluster does not
//! know a higher term took its office: it keeps accepting commands and keeps
//! sending its log. By the time those appends arrive, the receiver has moved
//! on, a newer leader has committed other entries at the same indices, and
//! the old leader's idea of what is committed is worth nothing.
use std::vec::Vec;

use super::*;

/// How many deposed leaders keep sending at once.
const KEPT: usize = 4;

impl<const N: usize> Cluster<N> {
    /// Keep `before` alive if the step just taken ended its leadership by
    /// moving its server into a higher term.
    pub(super) fn depose(&mut self, before: &Node<N>, seat: usize) {
        if before.role() != Role::Leader || self.nodes[seat].term() == before.term() {
            return;
        }
        if self.deposed.len() == KEPT {
            let forgotten = self.rng.below(KEPT);
            self.deposed.swap_remove(forgotten);
        }
        self.deposed.push(before.clone());
    }

    /// One deposed leader takes another command, then sends a slice of its
    /// log, under its old term, to a server that has left that term.
    ///
    /// The append is honest in everything but its age, except that half of
    /// them claim the whole of it is committed. Raft has one answer for it:
    /// `TermBehind`, in the receiver's term, with the receiver untouched,
    /// whatever the entries would have replaced.
    pub(super) fn haunt(&mut self, tally: &mut Tally) {
        let which = self.rng.below(self.deposed.len());
        self.command += 1;
        // A full log refuses; the old leader then sends what it has.
        let _full = self.deposed[which].propose(self.command);
        let ghost = self.deposed[which].clone();
        let (from, length) = (ghost.own(), ghost.last().index);
        let to = (from + 1 + self.rng.below(N - 1)) % N;
        if self.nodes[to].term() <= ghost.term() {
            return;
        }
        let start = self.rng.below(length + 1);
        let count = self.rng.below(length - start + 1);
        let inflated = self.rng.below(2) == 0;
        let request = append(
            from,
            ghost.term(),
            Position {
                index: start,
                term: term_at(&ghost, start),
            },
            if inflated { length } else { ghost.committed() },
        );
        let entries: Vec<Entry> = (start + 1..=start + count)
            .map(|index| ghost.entry(index).expect("held"))
            .collect();

        let before = self.nodes[to].clone();
        let replaced = (start + 1..=(start + count).min(before.committed()))
            .any(|index| before.entry(index) != ghost.entry(index));
        tally.haunted += 1;
        tally.haunted_committed += usize::from(replaced);
        tally.haunted_inflated += usize::from(inflated && length > self.chosen.len());

        let answer = self.nodes[to].append_entries(request, &entries);
        Self::reconciled(tally, &before, &self.nodes[to], request.term, &answer);
        let turned_away = Received {
            adopted: None,
            role: before.role(),
            outcome: AppendResponse {
                term: before.term(),
                append: Append::TermBehind,
            },
        };
        assert_eq!(answer, Ok(turned_away), "an old-term append was heard");
        assert_eq!(self.nodes[to], before, "an old-term append left a mark");
    }
}
