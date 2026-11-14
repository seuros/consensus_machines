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

  def test_an_array_subclass_is_refused
    list = Class.new(Array)
    raft = leader
    before = [raft.checkpoint, raft.role]
    error = assert_raises(TypeError) { Raft.new(list[10, 11, 12], 0) }
    assert_equal 'members must be an Array of Integers', error.message
    assert_raises(TypeError) { Raft.restored(list[10, 11, 12], 0, raft.checkpoint) }
    error = assert_raises(TypeError) { raft.append_entries(AppendRequest.new(1, 1, EMPTY, 0), list[]) }
    assert_equal 'entries must be an Array of ConsensusMachines::Entry', error.message
    assert_raises(TypeError) { Checkpoint.new(list[10, 11, 12], 0, 0, nil, 0, []) }
    assert_raises(TypeError) { Checkpoint.new([10, 11, 12], 0, 0, nil, 0, list[]) }
    assert_equal before, [raft.checkpoint, raft.role]
  end

  # Runs the block with every reader in `names` of `klass` raising.
  def with_raising_readers(klass, *names)
    names.each do |name|
      klass.alias_method(:"__original_#{name}", name)
      klass.define_method(name) { raise "a reader ran: #{name}" }
    end
    yield
  ensure
    names.each do |name|
      klass.alias_method(name, :"__original_#{name}")
      klass.remove_method(:"__original_#{name}")
    end
  end

  # Fields are read from the struct itself: a redefined reader is never run.
  def test_a_redefined_reader_is_ignored
    voter = node(3, 1)
    request = node.begin_election
    saved = leader.checkpoint
    with_raising_readers(VoteRequest, :candidate, :term, :last) do
      with_raising_readers(Position, :index, :term) do
        assert_equal :granted, voter.request_vote(request).outcome.vote
      end
    end
    with_raising_readers(Checkpoint, *Checkpoint.members) do
      assert_equal 1, Raft.restored(members, 0, saved).term
    end
    raft = node
    raft.begin_election
    with_raising_readers(VoteResponse, :term, :vote) do
      assert_equal :elected, raft.receive_vote(1, granted(1)).outcome
    end
  end

  def test_the_extension_alone_is_a_name_error
    lib = File.expand_path('../lib', __dir__)
    script = 'begin; require "consensus_machines_native/consensus_machines_native"; ' \
             'rescue NameError => e; print e.class; end'
    output = IO.popen([RbConfig.ruby, '-I', lib, '-e', script], err: %i[child out], &:read)
    assert_equal 'NameError', output
  end
end
