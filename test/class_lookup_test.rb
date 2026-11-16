# frozen_string_literal: true

require_relative 'test_helper'
require 'open3'
require 'rbconfig'

# The class of the value the extension wraps (Raft) is fixed when it loads.
# Nothing a caller does later, redefining the constant or `Object.const_get`,
# reaches a server being built. Each case runs in a fresh process: the class
# is fixed once per process, at load.
class ClassLookupTest < Minitest::Test
  LIB = File.expand_path('../lib', __dir__)

  PRELUDE = <<~RUBY
    require 'consensus_machines'
    C = ConsensusMachines
    members = [10, 11, 12]
  RUBY

  # Runs `body` after PRELUDE in a new Ruby; answers its last line of output.
  def isolated(body)
    out, err, status = Open3.capture3(RbConfig.ruby, '-I', LIB, '-e', PRELUDE + body)
    assert status.success?, "exited #{status.exitstatus}: #{err}"
    out.lines.last&.chomp
  end

  # Bug: a Raft constant redefined before the first server made it an
  # instance of the new class, without any of the gem's methods.
  def test_a_redefined_raft_constant_does_not_change_the_raft_class
    assert_equal 'true candidate 1 0', isolated(<<~RUBY)
      original = C::Raft
      C.send(:remove_const, :Raft)
      C.const_set(:Raft, Class.new)
      raft = original.new(members, 0)
      raft.begin_election
      puts [raft.instance_of?(original), raft.role, raft.term, raft.checkpoint.own].join(' ')
    RUBY
  end

  # Bug: building the first server ran `Object.const_get`, Ruby code in the
  # middle of a native call. No Ruby runs while one is built now.
  def test_building_a_raft_runs_no_const_get
    assert_equal '0 Raft Raft', isolated(<<~RUBY)
      hits = 0
      Object.define_singleton_method(:const_get) do |*args|
        hits += 1
        super(*args)
      end
      fresh = C::Raft.new(members, 0)
      restored = C::Raft.restored(members, 0, fresh.checkpoint)
      puts [hits, fresh.class.name.split('::').last, restored.class.name.split('::').last].join(' ')
    RUBY
  end

  # Bug: a redefined `Object.const_get` that raised hit an `unwrap` in the
  # extension: an uncatchable fatal, and the process exited.
  def test_a_raising_const_get_does_not_kill_the_process
    assert_equal 'follower 0', isolated(<<~RUBY)
      Object.define_singleton_method(:const_get) { |*| raise IOError, 'lookup ran while building' }
      raft = C::Raft.new(members, 0)
      puts [raft.role, raft.term].join(' ')
    RUBY
  end
end
