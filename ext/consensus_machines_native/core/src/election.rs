//! Candidate and voter: terms, ballots and the one vote per term.
use quorum_machines::{Acknowledgement, Quorum};

use crate::{Ballot, Denial, Raft, Received, Refusal, Role, Vote, VoteRequest, VoteResponse};

/// Count one member, translating the tracker's own refusal.
fn tally<const N: usize>(votes: &mut Quorum<N>, member: usize) -> Result<Acknowledgement, Refusal> {
    votes.acknowledge(member).map_err(|refusal| match refusal {
        quorum_machines::Refusal::UnknownMember => Refusal::UnknownMember,
    })
}

impl<const N: usize, const L: usize> Raft<N, L> {
    /// Start the next term as a candidate and vote for this server. Persist
    /// before broadcasting the request. A single-member cluster is its own
    /// majority and leads at once; read [`Self::role`].
    ///
    /// # Errors
    ///
    /// [`Refusal::TermExhausted`] at `u64::MAX`; nothing changed.
    pub fn begin_election(&mut self) -> Result<VoteRequest, Refusal> {
        let term = self
            .state
            .term
            .checked_add(1)
            .ok_or(Refusal::TermExhausted)?;
        let mut votes = self.votes.clone();
        votes.reset();
        match tally(&mut votes, self.state.own)? {
            Acknowledgement::Counted | Acknowledgement::Duplicate => {}
        }
        self.state.term = term;
        self.state.voted_for = Some(self.state.own);
        self.role = Role::Candidate;
        self.votes = votes;
        if self.votes.reached() {
            self.become_leader();
        }
        Ok(VoteRequest {
            candidate: self.state.own,
            term,
            last: self.last(),
        })
    }

    /// Answer an authenticated `RequestVote`. Persist before sending the reply.
    ///
    /// A higher term is adopted first and reported in `adopted`, whatever the
    /// ballot then is. A lower term is answered `Denied(TermBehind)`.
    ///
    /// # Errors
    ///
    /// [`Refusal::UnknownMember`], [`Refusal::OwnSeat`] for the candidate
    /// index; [`Refusal::Malformed`] for a zero term or a last position that
    /// cannot exist in the request's term. Nothing changed, no term adopted.
    pub fn request_vote(
        &mut self,
        request: VoteRequest,
    ) -> Result<Received<VoteResponse>, Refusal> {
        self.peer(request.candidate)?;
        let last = request.last;
        if request.term == 0 || last.term > request.term || (last.index == 0) != (last.term == 0) {
            return Err(Refusal::Malformed);
        }
        if request.term < self.term() {
            return Ok(self.ballot(None, Vote::Denied(Denial::TermBehind)));
        }
        let adopted = self.adopt(request.term);
        let vote = match self.state.voted_for {
            Some(seat) if seat == request.candidate => Vote::Duplicate,
            Some(_) => Vote::Denied(Denial::AlreadyVoted),
            None => {
                let local = self.last();
                if (last.term, last.index) >= (local.term, local.index) {
                    self.state.voted_for = Some(request.candidate);
                    self.role = Role::Follower;
                    Vote::Granted
                } else {
                    Vote::Denied(Denial::LogBehind)
                }
            }
        };
        Ok(self.ballot(adopted, vote))
    }

    /// Fold one authenticated ballot into the election.
    ///
    /// A higher term is adopted and answered [`Ballot::Overtaken`]. A leader
    /// still counts late grants of the term it won.
    ///
    /// # Errors
    ///
    /// [`Refusal::UnknownMember`], [`Refusal::OwnSeat`] for the peer index;
    /// [`Refusal::Stale`] for a response from a lower term, or one that says
    /// `Denied(TermBehind)` without being from a higher one;
    /// [`Refusal::NotCandidate`] for a ballot of the current term given to a
    /// follower. Nothing changed, no term adopted.
    pub fn receive_vote(
        &mut self,
        peer: usize,
        response: VoteResponse,
    ) -> Result<Received<Ballot>, Refusal> {
        self.peer(peer)?;
        if response.term > self.term() {
            let adopted = self.adopt(response.term);
            return Ok(self.received(adopted, Ballot::Overtaken));
        }
        if response.term < self.term() {
            return Err(Refusal::Stale);
        }
        match self.role {
            Role::Follower => return Err(Refusal::NotCandidate),
            Role::Candidate | Role::Leader => {}
        }
        let ballot = match response.vote {
            Vote::Denied(Denial::TermBehind) => return Err(Refusal::Stale),
            Vote::Denied(Denial::AlreadyVoted | Denial::LogBehind) => Ballot::Denied,
            Vote::Granted | Vote::Duplicate => match tally(&mut self.votes, peer)? {
                Acknowledgement::Duplicate => Ballot::Duplicate,
                Acknowledgement::Counted => self.counted(),
            },
        };
        Ok(self.received(None, ballot))
    }

    /// A newly counted grant: the one that wins the election, or not yet.
    fn counted(&mut self) -> Ballot {
        match self.role {
            Role::Candidate if self.votes.reached() => {
                self.become_leader();
                Ballot::Elected
            }
            Role::Candidate | Role::Leader | Role::Follower => Ballot::Counted,
        }
    }

    const fn ballot(&self, adopted: Option<u64>, vote: Vote) -> Received<VoteResponse> {
        self.received(
            adopted,
            VoteResponse {
                term: self.state.term,
                vote,
            },
        )
    }
}
