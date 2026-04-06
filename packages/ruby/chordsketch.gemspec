Gem::Specification.new do |s|
  s.name        = "chordsketch"
  s.version     = "0.1.0"
  s.summary     = "ChordPro file format parser and renderer"
  s.description = "Parse and render ChordPro files to text, HTML, and PDF. " \
                  "Native bindings via UniFFI for high performance."
  s.authors     = ["koedame"]
  s.license     = "MIT"
  s.homepage    = "https://github.com/koedame/chordsketch"

  s.metadata = {
    "source_code_uri"   => "https://github.com/koedame/chordsketch",
    "bug_tracker_uri"   => "https://github.com/koedame/chordsketch/issues",
    "changelog_uri"     => "https://github.com/koedame/chordsketch/releases",
    "rubygems_mfa_required" => "true",
  }

  s.required_ruby_version = ">= 3.0"

  s.files = Dir["lib/**/*.rb"] + Dir["lib/**/*.so"] + Dir["lib/**/*.dylib"] + Dir["lib/**/*.dll"]
  s.require_paths = ["lib"]

  s.add_dependency "ffi", "~> 1.15"
end
