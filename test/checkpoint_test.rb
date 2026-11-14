# frozen_string_literal: true
require_relative 'test_helper'
class CheckpointTest < Minitest::Test
  include RaftHelpers

  def test_construction_is_validated
    assert_raises(RangeError) { Raft.new([], 0) }
    assert_raises(RangeError) { node(10) }
    assert_raises(RangeError) { Raft.new([1, 1], 0) }
    assert_raises(RangeError) { node(3, 3) }
    assert_raises(TypeError) { Raft.new(nil, 0) }
    assert_raises(ArgumentError) { Raft.new([1]) }
    assert_raises(TypeError) { Raft.new([1.1], 0) }
    assert_raises(TypeError) { Raft.allocate }
  end

  def test_a_checkpoint_is_plain_data_that_can_be_rebuilt_from_its_fields
    follower = node(3, 1)
    follower.append_entries(AppendRequest.new(0, 3, EMPTY, 1), [Entry.new(1, 5), Entry.new(3, 6)])
    follower.request_vote(VoteRequest.new(2, 4, Position.new(2, 3)))
    saved = follower.checkpoint
    assert_equal Checkpoint.new(members, 1, 4, 2, 1, [Entry.new(1, 5), Entry.new(3, 6)]), saved
    assert_predicate saved, :frozen?
    # Through any serialization: the fields are Integers and Arrays of them.
    wire = Marshal.dump(saved.to_h.merge(entries: saved.entries.map(&:to_h)))
    fields = Marshal.load(wire)
    rebuilt = Checkpoint.new(**fields, entries: fields[:entries].map { Entry.new(**it) })
    restored = Raft.restored(members, 1, rebuilt)
    assert_equal saved, restored.checkpoint
    assert_equal :follower, restored.role
    assert_equal Entry.new(3, 6), restored.entry(2)
  end

  # Defect: every mutator call, refused ones included, bumped a revision that
  # made every outstanding checkpoint raise on restore.
  def test_a_checkpoint_is_not_staled_by_later_calls_refused_or_honoured
    raft = node
    saved = raft.checkpoint
    assert_equal :not_leader, raft.propose(1)
    assert_equal saved, raft.checkpoint
    assert_equal saved, Raft.restored(members, 0, saved).checkpoint
    raft.begin_election
    assert_equal 0, Raft.restored(members, 0, saved).term
    assert_equal 1, Raft.restored(members, 0, raft.checkpoint).term
  end

  def test_restore_loses_only_the_volatile_role_and_keeps_the_vote
    lead = leader
    lead.propose(9)
    restored = Raft.restored(members, 0, lead.checkpoint)
    assert_equal :follower, restored.role
    assert_equal lead.checkpoint, restored.checkpoint
    assert_equal :not_leader, restored.propose(1)
    voter = node(3, 2)
    assert_equal :granted, voter.request_vote(VoteRequest.new(0, 1, EMPTY)).outcome.vote
    voter = Raft.restored(members, 2, voter.checkpoint)
    again = voter.request_vote(VoteRequest.new(1, 1, EMPTY))
    assert_equal Denied.new(:already_voted), again.outcome.vote
    assert_equal :duplicate, voter.request_vote(VoteRequest.new(0, 1, EMPTY)).outcome.vote
  end

  def test_restore_refuses_another_membership_another_seat_and_impossible_states
    saved = node(3, 1).checkpoint
    assert_kind_of Raft, Raft.restored(members, 1, saved)
    assert_raises(RangeError) { Raft.restored(members, 0, saved) }
    assert_raises(RangeError) { Raft.restored([10, 11, 13], 1, saved) }
    assert_raises(RangeError) { Raft.restored(members(5), 1, saved) }
    # A state no server can be in does not exist as a Checkpoint at all.
    assert_raises(RangeError) { saved.with(voted_for: 3) }
    assert_raises(RangeError) { saved.with(committed: 1) }
    assert_raises(RangeError) { saved.with(entries: [Entry.new(1, 1)]) }
    assert_raises(RangeError) { saved.with(members: [10, 10, 12]) }
    assert_raises(RangeError) { saved.with(members: members(10)) }
    too_long = Array.new(65) { Entry.new(1, it) }
    assert_raises(RangeError) { saved.with(term: 1, entries: too_long) }
    assert_raises(RangeError) { Checkpoint.new(members, 3, 0, nil, 0, []) }
    # No server votes in term zero: every vote is cast in an election's term.
    assert_raises(RangeError) { saved.with(voted_for: 0) }
    assert_equal 0, saved.with(term: 1, voted_for: 0).voted_for
    assert_raises(TypeError) { Raft.restored(members, 1, saved.to_h) }
    assert_raises(TypeError) { saved.with(entries: [[1, 1]]) }
    assert_raises(TypeError) { saved.with(term: nil) }
    assert_raises(TypeError) { saved.with(members: 10) }
  end

  def test_restore_of_a_frozen_or_collected_source_is_independent
    lead = leader
    saved = lead.checkpoint
    lead = nil
    GC.start
    GC.compact
    restored = Raft.restored(members, 0, saved)
    assert_equal 1, restored.term
    assert_equal VoteRequest.new(0, 2, EMPTY), restored.begin_election
    assert_equal 1, saved.term
  end

  def test_a_checkpoint_holds_frozen_copies_of_its_arrays
    list = members
    entries = [Entry.new(1, 1)]
    saved = Checkpoint.new(list, 0, 1, nil, 0, entries)
    list << 99
    entries.clear
    assert_equal [members, [Entry.new(1, 1)]], [saved.members, saved.entries]
    assert_predicate saved.members, :frozen?
    assert_predicate saved.entries, :frozen?
  end
end
