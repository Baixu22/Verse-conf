# frozen_string_literal: true

require "json"

module VerseConf
  class Error < StandardError; end

  class VerseConf
    def initialize(wasm_instance, wasm_store)
      @instance = wasm_instance
      @store = wasm_store
    end

    def get_string(path)
      result = @instance.get_string(@store, path)
      result.null? ? nil : result.to_s
    end

    def get_number(path)
      result = @instance.get_number(@store, path)
      result.null? ? nil : result.to_f
    end

    def get_boolean(path)
      result = @instance.get_boolean(@store, path)
      result.null? ? nil : result.truthy?
    end

    def get_array(path)
      result = @instance.get_array(@store, path)
      result.null? ? nil : JSON.parse(result.to_s)
    end

    def get_object(path)
      result = @instance.get_object(@store, path)
      result.null? ? nil : JSON.parse(result.to_s)
    end

    def has_key?(path)
      @instance.has_key(@store, path)
    end

    def keys
      JSON.parse(@instance.keys(@store).to_s)
    end

    def to_json(*_args)
      @instance.to_json(@store)
    end

    def to_h
      JSON.parse(to_json)
    end
  end

  def self.parse(source, wasm_path: nil)
    wasm_bytes = load_wasm(wasm_path)

    engine = Wasmtime::Engine.new
    store = Wasmtime::Store.new(engine)
    module_ = Wasmtime::Module.new(engine, wasm_bytes)
    linker = Wasmtime::Linker.new(engine)
    linker.define_wasi!

    instance = linker.instantiate(store, module_)

    alloc_fn = instance.exports["alloc"]
    deallocate_fn = instance.exports["deallocate"]
    new_fn = instance.exports["new"]

    source_bytes = source.b
    source_ptr = alloc_fn.call(store, source_bytes.size)
    store.memory.write(source_ptr, source_bytes)

    result_ptr = new_fn.call(store, source_ptr, source_bytes.size)

    if result_ptr == 0
      raise Error, "Failed to parse VCF"
    end

    VerseConf.new(instance, store)
  ensure
    deallocate_fn&.call(store, source_ptr, source_bytes.size) if source_ptr
  end

  def self.load(path, wasm_path: nil)
    parse(File.read(path, encoding: "UTF-8"), wasm_path: wasm_path)
  end

  def self.load_wasm(wasm_path = nil)
    if wasm_path
      File.binread(wasm_path)
    else
      wasm_file = File.join(__dir__, "verseconf.wasm")
      File.binread(wasm_file)
    end
  end

  class << self
    alias_method :loads, :parse
    alias_method :load, :load
  end
end
