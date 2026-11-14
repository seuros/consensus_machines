# frozen_string_literal: true
require_relative 'test_helper'
class RaftTest < Minitest::Test
  include RaftHelpers

  def test_exact_membership_majority_for_every_supported_size
    assert_equal 1..9, Raft::MEMBERS
    assert_predicate Raft::MEMBERS, :frozen?
    Raft::MEMBERS.each do |count|
      raft = node(count)
      assert_equal VoteRequest.new(0, 1, EMPTY), raft.begin_election
      majority = count / 2 + 1
      (1...majority).each do |peer|
        assert_equal :candidate, raft.role
        tally = raft.receive_vote(peer, granted(1))
        expected = peer == majority - 1 ? Received.new(nil, :leader, :elected) : Received.new(nil, :candidate, :counted)
        assert_equal expected, tally
      end
      assert_equal :leader, raft.role, "exact quorum for #{count} actual peers"
      assert_equal 1, raft.propose(42)
      assert_equal(count == 1 ? 1 : 0, raft.committed)
      (1...majority).each do |peer|
        assert_equal Received.new(nil, :leader, :advanced), raft.acknowledge(peer, matched(1, 1))
      end
      assert_equal 1, raft.committed
      assert_equal Entry.new(1, 42), raft.entry(1)
    end
  end

  def test_readers_mirror_the_core
    raft = node(5, 2)
    assert_equal [10, 11, 12, 13, 14], raft.members
    assert_equal 2, raft.own
    assert_equal :follower, raft.role
    assert_equal 0, raft.term
    assert_equal 0, raft.committed
    assert_equal EMPTY, raft.last
    assert_nil raft.entry(0)
    assert_nil raft.entry(1)
  end

  def test_votes_report_the_term_stage_and_the_ballot_stage
    voter = node(3, 1)
    ask = VoteRequest.new(0, 2, EMPTY)
    assert_equal Received.new(2, :follower, VoteResponse.new(2, :granted)), voter.request_vote(ask)
    repeat = assert_unchanged(voter) { it.request_vote(ask) }
    assert_equal Received.new(nil, :follower, VoteResponse.new(2, :duplicate)), repeat
    taken = assert_unchanged(voter) { it.request_vote(VoteRequest.new(2, 2, EMPTY)) }
    assert_equal VoteResponse.new(2, Denied.new(:already_voted)), taken.outcome
    behind = assert_unchanged(voter) { it.request_vote(VoteRequest.new(2, 1, EMPTY)) }
    assert_equal Received.new(nil, :follower, VoteResponse.new(2, Denied.new(:term_behind))), behind
    voter.append_entries(AppendRequest.new(0, 2, EMPTY, 0), [Entry.new(2, 7)])
    # The higher term is adopted even though the vote is then denied.
    stale_log = voter.request_vote(VoteRequest.new(2, 5, EMPTY))
    assert_equal Received.new(5, :follower, VoteResponse.new(5, Denied.new(:log_behind))), stale_log
    assert_equal 5, voter.term
    assert_nil voter.checkpoint.voted_for
  end

  def test_ballots_count_distinct_members_and_a_higher_term_ends_the_campaign
    raft = node(5)
    raft.begin_election
    assert_equal Received.new(nil, :candidate, :counted), raft.receive_vote(1, granted(1))
    assert_equal :duplicate, assert_unchanged(raft) { it.receive_vote(1, granted(1)) }.outcome
    denied = VoteResponse.new(1, Denied.new(:already_voted))
    assert_equal :denied, assert_unchanged(raft) { it.receive_vote(3, denied) }.outcome
    assert_equal Received.new(nil, :leader, :elected), raft.receive_vote(2, VoteResponse.new(1, :duplicate))
    assert_equal Received.new(nil, :leader, :counted), raft.receive_vote(4, granted(1))
    ahead = VoteResponse.new(10, Denied.new(:term_behind))
    assert_equal Received.new(10, :follower, :overtaken), raft.receive_vote(1, ahead)
    assert_equal 10, raft.term
    assert_equal :follower, raft.role
  end

  def test_replication_reports_matches_conflicts_and_repeats
    lead = leader(5)
    assert_equal 1, lead.propose(99)
    follower = node(5, 1)
    request = AppendRequest.new(0, 1, EMPTY, 0)
    stored = follower.append_entries(request, [lead.entry(1)])
    assert_equal Received.new(1, :follower, matched(1, 1)), stored
    assert_equal Entry.new(1, 99), follower.entry(1)
    replay = assert_unchanged(follower) { it.append_entries(request, [lead.entry(1)]) }
    assert_equal Received.new(nil, :follower, matched(1, 1)), replay
    assert_refused(:malformed, follower) { it.append_entries(request, [Entry.new(2, 1)]) }
    missing = AppendRequest.new(0, 1, Position.new(2, 1), 0)
    conflict = assert_unchanged(follower) { it.append_entries(missing, []) }
    assert_equal AppendResponse.new(1, :conflict), conflict.outcome
    assert_equal :conflict, assert_unchanged(lead) { it.acknowledge(1, conflict.outcome) }.outcome
    assert_equal Received.new(nil, :leader, :advanced), lead.acknowledge(1, stored.outcome)
    assert_equal :duplicate, assert_unchanged(lead) { it.acknowledge(1, stored.outcome) }.outcome
    assert_equal 0, lead.committed
    assert_equal :advanced, lead.acknowledge(2, matched(1, 1)).outcome
    assert_equal 1, lead.committed
    beat = follower.append_entries(AppendRequest.new(0, 1, lead.last, 1), [])
    assert_equal matched(1, 1), beat.outcome
    assert_equal 1, follower.committed
    # A lower term is answered with the follower's own; a higher one ends a leadership.
    behind = assert_unchanged(follower) { it.append_entries(AppendRequest.new(2, 0 + 1, EMPTY, 0), []) }
    assert_equal matched(1, 0), behind.outcome
    follower.request_vote(VoteRequest.new(2, 4, Position.new(1, 1)))
    late = assert_unchanged(follower) { it.append_entries(request, []) }
    assert_equal Received.new(nil, :follower, AppendResponse.new(4, :term_behind)), late
    assert_equal Received.new(4, :follower, :overtaken), lead.acknowledge(1, late.outcome)
    assert_equal 4, lead.term
  end

  def test_bounded_log_holds_capacity_entries
    capacity = Raft::CAPACITY
    assert_equal 64, capacity
    raft = node(1)
    raft.begin_election
    assert_equal :leader, raft.role
    capacity.times { |i| assert_equal i + 1, raft.propose(i) }
    assert_refused(:log_full, raft) { it.propose(capacity + 1) }
    assert_nil raft.entry(capacity + 1)
    assert_equal capacity, raft.committed
    follower = node(3, 1)
    entries = Array.new(capacity) { Entry.new(1, it) }
    assert_equal matched(1, capacity), follower.append_entries(AppendRequest.new(0, 1, EMPTY, 0), entries).outcome
    assert_equal Position.new(capacity, 1), follower.last
  end
end
