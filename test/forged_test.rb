# frozen_string_literal: true
require_relative 'test_helper'

# Sealing stops accidents; the native side is the authority. A message forged
# past `initialize` raises TypeError or RangeError at conversion: it never
# reaches the server, so it never earns :malformed.
class ForgedTest < Minitest::Test
  include RaftHelpers

  VALUES = [Position, Entry, VoteRequest, Denied, VoteResponse, AppendRequest, Matched, AppendResponse, Received,
            Checkpoint].freeze

  def test_every_value_class_is_sealed
    VALUES.each do |value|
      assert_raises(NoMethodError) { value.allocate }
      assert_raises(TypeError) { Class.new(value) }
    end
  end

  # A named subclass still raises, but Ruby has bound the constant first: it
  # leaks. That is why sealing is not the authority.
  def test_a_named_subclass_raises_but_its_constant_leaks
    leaked(:LeakedResponse, VoteResponse)
    assert ForgedTest.const_defined?(:LeakedResponse, false)
    assert_operator ForgedTest::LeakedResponse, :<, VoteResponse
  end

  # A named subclass of `parent`: `class` raises TypeError, yet the constant
  # is bound. Given a `Data#initialize` bound past the check, it builds
  # values holding anything.
  def leaked(name, parent)
    unless ForgedTest.const_defined?(name, false)
      source = "class ForgedTest::#{name} < #{parent}; end"
      assert_raises(TypeError) { eval(source) } # rubocop:disable Security/Eval
    end
    klass = ForgedTest.const_get(name)
    klass.class_eval do
      define_method(:initialize) { |**fields| Data.instance_method(:initialize).bind_call(self, **fields) }
    end
    klass
  end

  def test_a_subclass_message_is_refused_whatever_it_holds
    raft = node
    raft.begin_election
    [0, 1].each do |term|
      response = leaked(:LeakedResponse, VoteResponse).new(term:, vote: :granted)
      assert_refused_raise(TypeError, raft) { raft.receive_vote(1, response) }
      append = leaked(:LeakedAppend, AppendResponse).new(term:, append: :conflict)
      assert_refused_raise(TypeError, raft) { raft.acknowledge(1, append) }
      request = leaked(:LeakedRequest, VoteRequest).new(candidate: 1, term:, last: EMPTY)
      assert_refused_raise(TypeError, raft) { raft.request_vote(request) }
      entry = leaked(:LeakedEntry, Entry).new(term:, command: 1)
      assert_refused_raise(TypeError, raft) { raft.append_entries(AppendRequest.new(1, 1, EMPTY, 0), [entry]) }
    end
    position = leaked(:LeakedPosition, Position).new(index: 5, term: 0)
    assert_raises(TypeError) { AppendRequest.new(1, 1, position, 0) }
  end

  # The forgeries the review probed: each answered :malformed before.
  def test_a_marshalled_message_is_a_range_error_never_malformed
    raft = node
    raft.begin_election
    {
      -> { raft.receive_vote(1, forge(VoteResponse.new(1, :granted), "i\x06".b, "i\x00".b)) } => RangeError,
      -> { raft.receive_vote(1, forge(VoteResponse.new(2, Denied.new(:term_behind)), "i\x07".b, "i\x06".b)) } =>
        RangeError,
      -> { raft.acknowledge(1, forge(AppendResponse.new(1, :conflict), "i\x06".b, "i\x00".b)) } => RangeError,
      -> { raft.request_vote(forge(VoteRequest.new(7, 1, EMPTY), "i\x06".b, "i\x00".b)) } => RangeError,
      -> { raft.append_entries(forge(AppendRequest.new(7, 1, EMPTY, 0), "i\x06".b, "i\x00".b), []) } => RangeError,
      -> { raft.append_entries(AppendRequest.new(1, 1, EMPTY, 0), [forge(Entry.new(1, 9), "i\x06".b, "i\x00".b)]) } =>
        RangeError
    }.each { |call, error| assert_refused_raise(error, raft, &call) }
  end

  def test_a_marshalled_checkpoint_is_validated_again
    saved = forge(Checkpoint.new(members, 0, 1, 1, 0, []), "i\x06".b, "i\x00".b)
    assert_equal [0, 1], [saved.term, saved.voted_for]
    assert_raises(RangeError) { Raft.restored(members, 0, saved) }
  end

  # The call raises `error` and the server is exactly as it was.
  def assert_refused_raise(error, raft, &)
    before = [raft.checkpoint, raft.role]
    assert_raises(error, &)
    assert_equal before, [raft.checkpoint, raft.role]
  end

  # A singleton class on the instance itself does not change what it is.
  def test_a_value_with_its_own_singleton_class_is_still_this_gems
    raft = node
    raft.begin_election
    assert_equal :elected, raft.receive_vote(1, granted(1).tap(&:singleton_class)).outcome
  end
end
