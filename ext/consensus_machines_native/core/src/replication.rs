//! Leader and follower: proposals, log matching and commitment.
use crate::{
    Ack, Append, AppendRequest, AppendResponse, Entry, Raft, Received, Refusal, Role, log::Plan,
};

impl<const N: usize, const L: usize> Raft<N, L> {
    /// Add a current-term command identity and answer its index. Persist
    /// before replication or application.
    ///
    /// # Errors
    ///
    /// [`Refusal::NotLeader`]; [`Refusal::LogFull`]. Nothing changed.
    pub fn propose(&mut self, command: u64) -> Result<usize, Refusal> {
        self.leading()?;
        let entry = Entry {
            term: self.term(),
            command,
        };
        let index = self.state.log.push(entry).ok_or(Refusal::LogFull)?;
        self.matched[self.state.own] = index;
        self.advance_commit();
        Ok(index)
    }

    /// Answer an authenticated `AppendEntries`. Persist before sending the reply.
    ///
    /// A higher term is adopted first and reported in `adopted`, whatever
    /// becomes of the entries. A lower term is answered `TermBehind`. A
    /// candidate of the same term becomes a follower. An uncommitted suffix
    /// that disagrees with the leader is replaced; a committed entry never is.
    ///
    /// # Errors
    ///
    /// [`Refusal::UnknownMember`], [`Refusal::OwnSeat`] for the leader index;
    /// [`Refusal::Malformed`]; [`Refusal::TooManyEntries`]; and, unless the
    /// request is from a lower term, [`Refusal::RivalLeader`],
    /// [`Refusal::Mismatch`], [`Refusal::CommittedConflict`]. Nothing changed,
    /// no term adopted.
    pub fn append_entries(
        &mut self,
        request: AppendRequest,
        entries: &[Entry],
    ) -> Result<Received<AppendResponse>, Refusal> {
        self.peer(request.leader)?;
        Self::well_formed(request, entries)?;
        if request.term < self.term() {
            return Ok(self.appended(None, Append::TermBehind));
        }
        if request.term == self.term() {
            match self.role {
                Role::Leader => return Err(Refusal::RivalLeader),
                Role::Follower | Role::Candidate => {}
            }
        }
        let previous = request.previous;
        let plan = self
            .state
            .log
            .plan(previous, entries, self.state.committed)?;
        let adopted = self.adopt(request.term);
        self.role = Role::Follower;
        let append = match plan {
            Plan::Conflict => Append::Conflict,
            Plan::Matched { rewrite } => {
                if let Some(offset) = rewrite {
                    self.state.log.rewrite(previous.index, entries, offset);
                }
                let end = previous.index + entries.len();
                self.state.committed = self.state.committed.max(request.committed.min(end));
                Append::Matched(end)
            }
        };
        Ok(self.appended(adopted, append))
    }

    /// Fold one authenticated append reply into the leader's progress.
    ///
    /// The reply is checked with [`AppendResponse::new`] before its term is
    /// looked at, so a reply no follower can send never moves this server
    /// into its term. A higher term is then adopted and answered
    /// [`Ack::Overtaken`].
    ///
    /// # Errors
    ///
    /// [`Refusal::UnknownMember`], [`Refusal::OwnSeat`] for the peer index;
    /// [`Refusal::Malformed`] for a reply [`AppendResponse::new`] rejects (a
    /// zero term, or `TermBehind` below term two);
    /// [`Refusal::Stale`] for a response from a lower term, or one that says
    /// `TermBehind` without being from a higher one; [`Refusal::NotLeader`]
    /// for a response of the current term given to a server that does not lead
    /// it; [`Refusal::BeyondLog`] for a match past the end of the log. Nothing
    /// changed, no term adopted.
    pub fn acknowledge(
        &mut self,
        peer: usize,
        response: AppendResponse,
    ) -> Result<Received<Ack>, Refusal> {
        self.peer(peer)?;
        let response =
            AppendResponse::new(response.term, response.append).ok_or(Refusal::Malformed)?;
        if let Some(adopted) = self.reply_term(response.term)? {
            return Ok(self.received(Some(adopted), Ack::Overtaken));
        }
        self.leading()?;
        let ack = match response.append {
            Append::TermBehind => return Err(Refusal::Stale),
            Append::Conflict => Ack::Conflict,
            Append::Matched(index) if index > self.state.log.len() => {
                return Err(Refusal::BeyondLog);
            }
            Append::Matched(index) if index <= self.matched[peer] => Ack::Duplicate,
            Append::Matched(index) => {
                self.matched[peer] = index;
                self.advance_commit();
                Ack::Advanced
            }
        };
        Ok(self.received(None, ack))
    }

    /// The checks a request passes or fails on its own, whatever this server
    /// holds.
    fn well_formed(request: AppendRequest, entries: &[Entry]) -> Result<(), Refusal> {
        let request = AppendRequest::new(
            request.leader,
            request.term,
            request.previous,
            request.committed,
        )
        .ok_or(Refusal::Malformed)?;
        let previous = request.previous;
        let mut floor = previous.term;
        for entry in entries {
            let entry = Entry::new(entry.term, entry.command).ok_or(Refusal::Malformed)?;
            if entry.term < floor || entry.term > request.term {
                return Err(Refusal::Malformed);
            }
            floor = entry.term;
        }
        let fits = previous
            .index
            .checked_add(entries.len())
            .is_some_and(|end| end <= L);
        if entries.is_empty() || fits {
            Ok(())
        } else {
            Err(Refusal::TooManyEntries)
        }
    }

    const fn appended(&self, adopted: Option<u64>, append: Append) -> Received<AppendResponse> {
        self.received(
            adopted,
            AppendResponse {
                term: self.state.term,
                append,
            },
        )
    }
}
