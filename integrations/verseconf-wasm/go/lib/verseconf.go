package verseconf

import (
	"errors"
	"os"
	"wasmtime"
)

type VerseConf struct {
	instance *wasmtime.Instance
	store    *wasmtime.Store
}

func Parse(source string, wasmPath string) (*VerseConf, error) {
	engine, err := wasmtime.NewEngine()
	if err != nil {
		return nil, err
	}

	store := wasmtime.NewStore(engine)

	linker := wasmtime.NewLinker(engine)
	if err := linker.DefineWasi(); err != nil {
		return nil, err
	}

	var wasmData []byte
	if wasmPath != "" {
		wasmData, err = os.ReadFile(wasmPath)
	} else {
		wasmData, err = LoadDefaultWasm()
	}
	if err != nil {
		return nil, err
	}

	module, err := wasmtime.NewModule(engine, wasmData)
	if err != nil {
		return nil, err
	}

	instance, err := linker.Instantiate(store, module)
	if err != nil {
		return nil, err
	}

	newFunc := instance.GetExport(store, "new")
	if newFunc == nil {
		return nil, errors.New("failed to find 'new' export")
	}

	allocFunc := instance.GetExport(store, "alloc")
	deallocateFunc := instance.GetExport(store, "deallocate")

	sourceBytes := []byte(source)
	sourcePtr, err := allocFunc.Func().Call(store, len(sourceBytes))
	if err != nil {
		return nil, err
	}

	memory := instance.GetExport(store, "memory")
	if memory == nil {
		return nil, errors.New("failed to find memory export")
	}

	mem := memory.Memory()
	mem.Write(store, uint(sourcePtr.(int64)), sourceBytes)

	resultPtr, err := newFunc.Func().Call(store, sourcePtr, len(sourceBytes))
	if err != nil {
		return nil, err
	}

	if resultPtr.(int64) == 0 {
		return nil, errors.New("failed to parse VCF")
	}

	deallocateFunc.Func().Call(store, sourcePtr, len(sourceBytes))

	return &VerseConf{
		instance: instance,
		store:    store,
	}, nil
}

func LoadDefaultWasm() ([]byte, error) {
	return nil, errors.New("no embedded wasm, please specify wasmPath")
}

func LoadWasmFile(path string) ([]byte, error) {
	return os.ReadFile(path)
}

func (v *VerseConf) GetString(path string) (string, bool) {
	getString := v.instance.GetExport(v.store, "get_string")
	if getString == nil {
		return "", false
	}

	resultPtr := getString.Func().Call(v.store, path)
	if resultPtr == nil {
		return "", false
	}

	ptr := resultPtr.(int64)
	if ptr == 0 {
		return "", false
	}

	memory := v.instance.GetExport(v.store, "memory")
	if memory == nil {
		return "", false
	}

	mem := memory.Memory()
	data := mem.Read(v.store, uint(ptr), 1000)

	for i, b := range data {
		if b == 0 {
			return string(data[:i]), true
		}
	}

	return string(data), true
}

func (v *VerseConf) GetNumber(path string) (float64, bool) {
	getNumber := v.instance.GetExport(v.store, "get_number")
	if getNumber == nil {
		return 0, false
	}

	result := getNumber.Func().Call(v.store, path)
	if result == nil {
		return 0, false
	}

	return result.(float64), true
}

func (v *VerseConf) GetBoolean(path string) (bool, bool) {
	getBoolean := v.instance.GetExport(v.store, "get_boolean")
	if getBoolean == nil {
		return false, false
	}

	result := getBoolean.Func().Call(v.store, path)
	if result == nil {
		return false, false
	}

	return result.(int64) != 0, true
}

func (v *VerseConf) HasKey(path string) bool {
	hasKey := v.instance.GetExport(v.store, "has_key")
	if hasKey == nil {
		return false
	}

	result := hasKey.Func().Call(v.store, path)
	return result.(int64) != 0
}

func (v *VerseConf) ToJSON() string {
	toJson := v.instance.GetExport(v.store, "to_json")
	if toJson == nil {
		return "{}"
	}

	resultPtr := toJson.Func().Call(v.store)
	if resultPtr == nil {
		return "{}"
	}

	ptr := resultPtr.(int64)
	if ptr == 0 {
		return "{}"
	}

	memory := v.instance.GetExport(v.store, "memory")
	if memory == nil {
		return "{}"
	}

	mem := memory.Memory()
	data := mem.Read(v.store, uint(ptr), 10000)

	for i, b := range data {
		if b == 0 {
			return string(data[:i])
		}
	}

	return string(data)
}

func (v *VerseConf) Validate() bool {
	validate := v.instance.GetExport(v.store, "validate")
	if validate == nil {
		return false
	}

	result := validate.Func().Call(v.store)
	return result.(int64) != 0
}
