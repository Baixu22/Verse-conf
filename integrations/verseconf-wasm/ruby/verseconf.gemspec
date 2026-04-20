Gem::Specification.new do |spec|
  spec.name = "verseconf"
  spec.version = "0.1.0"
  spec.authors = ["VerseConf Team"]
  spec.summary = "VerseConf - Modern configuration format for the AI era"
  spec.description = "Ruby bindings for VerseConf using WebAssembly"
  spec.license = ["MIT", "Apache-2.0"]
  spec.files = Dir.glob("{lib}/**/*.rb")
  spec.homepage = "https://github.com/Baixu22/Verse-conf"
  spec.required_ruby_version = ">= 3.0"

  spec.add_dependency "wasmtime-ruby", "~> 15.0"
end
