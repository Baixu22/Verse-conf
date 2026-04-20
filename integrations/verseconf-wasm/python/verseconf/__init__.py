"""
VerseConf Python bindings using WASM
"""

from wasmtime import Module, Instance, Store, Linker, WasiConfig
import json
from pathlib import Path
from typing import Any, Dict, List, Optional, Union


class VerseConfError(Exception):
    """VerseConf parsing or validation error"""
    pass


class VerseConf:
    """Parsed VerseConf configuration"""

    def __init__(self, instance: Instance, store: Store):
        self._instance = instance
        self._store = store
        self._get_string = instance.exports[store]["get_string"]
        self._get_number = instance.exports[store]["get_number"]
        self._get_boolean = instance.exports[store]["get_boolean"]
        self._get_array = instance.exports[store]["get_array"]
        self._get_object = instance.exports[store]["get_object"]
        self._has_key = instance.exports[store]["has_key"]
        self._keys = instance.exports[store]["keys"]
        self._to_json = instance.exports[store]["to_json"]

    def get_string(self, path: str) -> Optional[str]:
        """Get a string value at the given path"""
        result = self._get_string(self._store, path)
        if result is None:
            return None
        return result

    def get_number(self, path: str) -> Optional[float]:
        """Get a number value at the given path"""
        result = self._get_number(self._store, path)
        if result is None:
            return None
        return float(result)

    def get_boolean(self, path: str) -> Optional[bool]:
        """Get a boolean value at the given path"""
        result = self._get_boolean(self._store, path)
        if result is None:
            return None
        return bool(result)

    def get_array(self, path: str) -> Optional[List[Any]]:
        """Get an array value at the given path"""
        result = self._get_array(self._store, path)
        if result is None:
            return None
        return list(result)

    def get_object(self, path: str) -> Optional[Dict[str, Any]]:
        """Get an object value at the given path"""
        result = self._get_object(self._store, path)
        if result is None:
            return None
        return dict(result)

    def has_key(self, path: str) -> bool:
        """Check if a key exists at the given path"""
        return bool(self._has_key(self._store, path))

    def keys(self) -> List[str]:
        """Get all top-level keys"""
        return list(self._keys(self._store))

    def to_json(self) -> str:
        """Convert to JSON string"""
        return self._to_json(self._store)

    def to_dict(self) -> Dict[str, Any]:
        """Convert to Python dictionary"""
        return json.loads(self.to_json())


def _load_wasm_module(wasm_path: Optional[str] = None) -> bytes:
    """Load the WASM module from file or use bundled version"""
    if wasm_path:
        return Path(wasm_path).read_bytes()

    current_dir = Path(__file__).parent
    wasm_path = current_dir / "verseconf.wasm"

    if wasm_path.exists():
        return wasm_path.read_bytes()

    raise FileNotFoundError(
        f"VerseConf WASM module not found. "
        f"Please build with: wasm-pack build --target wasm32-wasi"
    )


def parse(source: str, wasm_path: Optional[str] = None) -> VerseConf:
    """
    Parse a VerseConf configuration string

    Args:
        source: The VerseConf configuration content
        wasm_path: Optional path to the WASM module

    Returns:
        A VerseConf object for accessing the parsed configuration

    Raises:
        VerseConfError: If parsing fails
    """
    wasm_bytes = _load_wasm_module(wasm_path)

    wasi_config = WasiConfig()
    wasi_config.inherit_stdout()
    wasi_config.inherit_stderr()

    engine = Engine()
    store = Store(engine)
    store.set_wasi(wasi_config)

    linker = Linker(engine)
    linker.define_wasi()

    module = Module(engine, wasm_bytes)
    instance = linker.instantiate(store, module)

    new_fn = instance.exports[store]["new"]
    memory = instance.exports[store]["memory"]

    store = Store(engine)
    store.set_wasi(wasi_config)

    instance = linker.instantiate(store, module)

    alloc_fn = instance.exports[store]["alloc"]
    deallocate_fn = instance.exports[store]["deallocate"]

    source_bytes = source.encode("utf-8")
    source_ptr = alloc_fn(store, len(source_bytes))
    memory.write(store, source_bytes, source_ptr)

    try:
        result_ptr = new_fn(store, source_ptr, len(source_bytes))
        if result_ptr == 0:
            error_ptr = instance.exports[store]["get_last_error"](store)
            if error_ptr != 0:
                error_len = instance.exports[store]["get_last_error_length"](store)
                error_bytes = memory.read(store, error_ptr, error_len)
                error_msg = error_bytes.decode("utf-8")
                deallocate_fn(store, error_ptr, error_len)
                raise VerseConfError(error_msg)

        return VerseConf(instance, store)

    finally:
        deallocate_fn(store, source_ptr, len(source_bytes))


def loads(source: str, wasm_path: Optional[str] = None) -> Dict[str, Any]:
    """
    Parse a VerseConf string and return as a Python dictionary

    Args:
        source: The VerseConf configuration content
        wasm_path: Optional path to the WASM module

    Returns:
        A dictionary representing the parsed configuration

    Raises:
        VerseConfError: If parsing fails
    """
    conf = parse(source, wasm_path)
    return conf.to_dict()


def load(path: Union[str, Path], wasm_path: Optional[str] = None) -> Dict[str, Any]:
    """
    Load and parse a VerseConf file

    Args:
        path: Path to the .vcf file
        wasm_path: Optional path to the WASM module

    Returns:
        A dictionary representing the parsed configuration

    Raises:
        VerseConfError: If parsing fails
        FileNotFoundError: If the file doesn't exist
    """
    source = Path(path).read_text(encoding="utf-8")
    return loads(source, wasm_path)


__version__ = "0.1.0"

__all__ = ["parse", "loads", "load", "VerseConf", "VerseConfError"]
