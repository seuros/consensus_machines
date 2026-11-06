# frozen_string_literal: true
require_relative 'lib/consensus_machines/version'
Gem::Specification.new do |spec|
  spec.name = 'consensus_machines'
  spec.version = ConsensusMachines::VERSION
  spec.authors = ['Abdelkader Boudih']
  spec.email = ['oss@seuros.com']
  spec.summary = 'Native bounded Raft transition model for Ruby'
  spec.description = 'A thin bridge to an allocation-free fixed-membership Rust Raft model. ' \
                     'Not a production consensus service.'
  spec.homepage = 'https://github.com/seuros/consensus_machines'
  spec.license = 'MIT'
  spec.required_ruby_version = '>= 4.0.0'
  spec.metadata['source_code_uri'] = spec.homepage
  spec.metadata['bug_tracker_uri'] = "#{spec.homepage}/issues"
  spec.metadata['rubygems_mfa_required'] = 'true'
  spec.metadata['cargo_crate_name'] = 'consensus_machines_native'
  spec.metadata['cargo_manifest_path'] = 'ext/consensus_machines_native/ffi/Cargo.toml'
  spec.files = Dir.glob(%w[
    lib/**/*.rb sig/**/*.rbs ext/**/*.{rb,rs,toml,md} Cargo.toml
    Cargo.lock README.md CHANGELOG.md LICENSE.txt
  ]).select { |path| File.file?(path) }
  spec.require_paths = ['lib']
  spec.extensions = ['ext/consensus_machines_native/extconf.rb']
  spec.add_dependency 'rb_sys', '~> 0.9', '>= 0.9.130'
end
