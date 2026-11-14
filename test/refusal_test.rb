# frozen_string_literal: true
require_relative 'test_helper'
# Every refusal symbol of every method. A refusal changes nothing, adopts no
# term even when the message carries a higher one, and never raises.
class RefusalTest < Minitest::Test
  include RaftHelpers

  HIGH = 9

  def test_begin_election_refuses_only_the_last_term
    last = Checkpoint.new(members, 0, 2**64 - 1, nil, 0, [])
    raft = Raft.restored(members, 0, last)
    assert_refused(:term_exhausted, raft, &:begin_election)
  end

  def test_request_vote_refusals
    voter = node(3, 1)
    {
      VoteRequest.new(3, HIGH, EMPTY) => :unknown_member,
      VoteRequest.new(1, HIGH, EMPTY) => :own_seat
    }.each { |request, refusal| assert_refused(refusal, voter) { it.request_vote(request) } }
    assert_equal 0, voter.term
    # The core answers :malformed to such a request; from Ruby it cannot be built.
    assert_raises(RangeError) { VoteRequest.new(0, 0, EMPTY) }
    assert_raises(RangeError) { VoteRequest.new(0, HIGH, Position.new(HIGH + 1, HIGH + 1)) }
  end

  def test_receive_vote_refusals
    raft = node
    raft.request_vote(VoteRequest.new(2, 1, EMPTY))
    assert_refused(:not_candidate, raft) { it.receive_vote(1, granted(1)) }
    raft.begin_election
    assert_refused(:unknown_member, raft) { it.receive_vote(3, granted(HIGH)) }
    assert_refused(:own_seat, raft) { it.receive_vote(0, granted(HIGH)) }
    assert_refused(:stale, raft) { it.receive_vote(1, granted(1)) }
    assert_refused(:stale, raft) { it.receive_vote(1, VoteResponse.new(2, Denied.new(:term_behind))) }
    assert_equal 2, raft.term
  end

  def test_propose_refusals
    raft = node
    assert_refused(:not_leader, raft) { it.propose(1) }
    raft.begin_election
    assert_refused(:not_leader, raft) { it.propose(1) }
  end

  def test_append_entries_refusals
    follower = node(3, 1)
    follower.append_entries(AppendRequest.new(0, 1, EMPTY, 1), [Entry.new(1, 7)])
    high = AppendRequest.new(0, HIGH, EMPTY, 0)
    [
      [:unknown_member, AppendRequest.new(3, HIGH, EMPTY, 0), []],
      [:own_seat, AppendRequest.new(1, HIGH, EMPTY, 0), []],
      [:malformed, high, [Entry.new(HIGH + 1, 1)]],
      [:malformed, high, [Entry.new(2, 1), Entry.new(1, 2)]],
      [:too_many_entries, high, Array.new(65) { Entry.new(HIGH, it) }],
      [:too_many_entries, AppendRequest.new(0, HIGH, Position.new(64, 1), 0), [Entry.new(HIGH, 1)]],
      [:mismatch, high, [Entry.new(1, 8)]],
      [:committed_conflict, high, [Entry.new(HIGH, 7)]]
    ].each do |refusal, request, entries|
      assert_refused(refusal, follower) { it.append_entries(request, entries) }
    end
    assert_equal 1, follower.term
    assert_raises(RangeError) { AppendRequest.new(0, 0, EMPTY, 0) }
    assert_raises(RangeError) { AppendRequest.new(0, 1, Position.new(1, 2), 0) }
    assert_refused(:rival_leader, leader) { it.append_entries(AppendRequest.new(1, 1, EMPTY, 0), []) }
  end

  def test_acknowledge_refusals
    raft = leader
    raft.propose(7)
    raft.begin_election
    raft.receive_vote(1, granted(2))
    assert_equal :leader, raft.role
    assert_refused(:unknown_member, raft) { it.acknowledge(3, matched(HIGH, 1)) }
    assert_refused(:own_seat, raft) { it.acknowledge(0, matched(HIGH, 1)) }
    assert_refused(:stale, raft) { it.acknowledge(1, matched(1, 1)) }
    assert_refused(:stale, raft) { it.acknowledge(1, AppendResponse.new(2, :term_behind)) }
    assert_refused(:beyond_log, raft) { it.acknowledge(1, matched(2, 2)) }
    assert_equal [2, 0], [raft.term, raft.committed]
    follower = node(3, 1)
    follower.request_vote(VoteRequest.new(2, 1, EMPTY))
    assert_refused(:not_leader, follower) { it.acknowledge(0, matched(1, 0)) }
  end

  def test_malformed_ruby_input_raises_before_any_transition
    raft = leader
    saved = raft.checkpoint
    assert_raises(TypeError) { raft.propose(1.5) }
    assert_raises(RangeError) { raft.propose(-1) }
    assert_raises(RangeError) { raft.propose(2**80) }
    assert_raises(TypeError) { raft.request_vote([1, 1, 0, 0]) }
    assert_raises(TypeError) { raft.request_vote(VoteRequest.new(1, 1, [0, 0])) }
    assert_raises(TypeError) { raft.request_vote(VoteRequest.new(1, '1', EMPTY)) }
    assert_raises(TypeError) { raft.receive_vote(1, [1, :granted]) }
    assert_raises(TypeError) { raft.receive_vote(1, VoteResponse.new(1, 1)) }
    assert_raises(TypeError) { raft.receive_vote(1, VoteResponse.new(1, true)) }
    assert_raises(RangeError) { raft.receive_vote(1, VoteResponse.new(1, :maybe)) }
    assert_raises(RangeError) { raft.receive_vote(1, VoteResponse.new(1, Denied.new(:mood))) }
    request = AppendRequest.new(1, 10, EMPTY, 0)
    assert_raises(TypeError) { raft.append_entries(request, nil) }
    assert_raises(TypeError) { raft.append_entries(request, [[10, 1]]) }
    assert_raises(TypeError) { raft.append_entries(request, [Entry.new(10, 1), Entry.new(10, 'bad')]) }
    assert_raises(TypeError) { raft.append_entries(AppendRequest.new(1, 10, [0, 0], 0), []) }
    assert_raises(TypeError) { raft.acknowledge(1, [1, 1]) }
    assert_raises(RangeError) { raft.acknowledge(1, AppendResponse.new(10, :matched)) }
    assert_raises(RangeError) { raft.acknowledge(1, AppendResponse.new(10, :perhaps)) }
    assert_raises(TypeError) { raft.acknowledge(1, AppendResponse.new(10, Matched.new('1'))) }
    assert_raises(TypeError) { raft.entry('1') }
    assert_equal [saved, :leader], [raft.checkpoint, raft.role]
  end

  def test_entries_are_never_coerced
    raft = node
    called = false
    coercible = Object.new
    coercible.define_singleton_method(:to_ary) { called = true }
    coercible.define_singleton_method(:term) { called = true }
    assert_raises(TypeError) { raft.append_entries(AppendRequest.new(1, 1, EMPTY, 0), [coercible]) }
    refute called
    assert_equal 0, raft.term
  end

  def test_frozen_server_raises_on_every_mutating_call_and_still_reads
    raft = leader
    saved = raft.checkpoint
    raft.freeze
    assert_raises(FrozenError) { raft.begin_election }
    assert_raises(FrozenError) { raft.request_vote(VoteRequest.new(1, 2, EMPTY)) }
    assert_raises(FrozenError) { raft.receive_vote(1, granted(1)) }
    assert_raises(FrozenError) { raft.propose(1) }
    assert_raises(FrozenError) { raft.append_entries(AppendRequest.new(1, 2, EMPTY, 0), []) }
    assert_raises(FrozenError) { raft.acknowledge(1, matched(1, 0)) }
    # Frozen is checked before the arguments are converted.
    assert_raises(FrozenError) { raft.request_vote(Object.new) }
    assert_raises(FrozenError) { raft.receive_vote(-1, nil) }
    assert_raises(FrozenError) { raft.propose('1') }
    assert_raises(FrozenError) { raft.append_entries(nil, Class.new(Array).new) }
    assert_raises(FrozenError) { raft.acknowledge(0.5, :conflict) }
    assert_equal [saved, :leader, 1, EMPTY], [raft.checkpoint, raft.role, raft.term, raft.last]
  end
end
