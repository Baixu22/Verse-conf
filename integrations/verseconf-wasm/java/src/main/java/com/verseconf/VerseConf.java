package com.verseconf;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.Paths;
import java.util.Optional;
import java.util.function.Consumer;

public class VerseConf implements AutoCloseable {
    private Object wasmInstance;
    private String lastError;

    public static VerseConf parse(String source) throws VerseConfException {
        return parse(source, findWasmPath());
    }

    public static VerseConf parse(String source, String wasmPath) throws VerseConfException {
        if (wasmPath == null || wasmPath.isEmpty()) {
            wasmPath = findWasmPath();
        }
        if (wasmPath == null) {
            throw new VerseConfException("Could not find verseconf_wasm WASM file");
        }
        try {
            byte[] wasmBytes = Files.readAllBytes(Paths.get(wasmPath));
            return new VerseConf(source, wasmBytes);
        } catch (IOException e) {
            throw new VerseConfException("Failed to read WASM file: " + wasmPath, e);
        }
    }

    private static String findWasmPath() {
        String[] paths = {
            "../pkg/verseconf_wasm_bg.wasm",
            "pkg/verseconf_wasm_bg.wasm",
            "verseconf-wasm/pkg/verseconf_wasm_bg_bg.wasm",
            System.getProperty("user.dir") + "/pkg/verseconf_wasm_bg.wasm"
        };

        for (String path : paths) {
            Path p = Paths.get(path);
            if (Files.exists(p)) {
                return path;
            }
        }
        return null;
    }

    private VerseConf(String source, byte[] wasmBytes) throws VerseConfException {
        try {
            this.wasmInstance = initWasm(wasmBytes, source);
        } catch (Exception e) {
            throw new VerseConfException("Failed to initialize WASM", e);
        }
    }

    private native Object initWasm(byte[] wasmBytes, String source) throws Exception;

    public String getString(String path) {
        if (wasmInstance == null) {
            return null;
        }
        try {
            return nativeGetString(wasmInstance, path);
        } catch (Exception e) {
            lastError = e.getMessage();
            return null;
        }
    }

    private native String nativeGetString(Object instance, String path);

    public Optional<Double> getNumber(String path) {
        if (wasmInstance == null) {
            return Optional.empty();
        }
        try {
            double result = nativeGetNumber(wasmInstance, path);
            return Double.isNaN(result) ? Optional.empty() : Optional.of(result);
        } catch (Exception e) {
            lastError = e.getMessage();
            return Optional.empty();
        }
    }

    private native double nativeGetNumber(Object instance, String path);

    public Optional<Boolean> getBoolean(String path) {
        if (wasmInstance == null) {
            return Optional.empty();
        }
        try {
            return nativeGetBoolean(wasmInstance, path) ? Optional.of(true) : Optional.of(false);
        } catch (Exception e) {
            lastError = e.getMessage();
            return Optional.empty();
        }
    }

    private native boolean nativeGetBoolean(Object instance, String path);

    public boolean hasKey(String path) {
        if (wasmInstance == null) {
            return false;
        }
        try {
            return nativeHasKey(wasmInstance, path);
        } catch (Exception e) {
            return false;
        }
    }

    private native boolean nativeHasKey(Object instance, String path);

    public String toJson() {
        if (wasmInstance == null) {
            return "{}";
        }
        try {
            return nativeToJson(wasmInstance);
        } catch (Exception e) {
            lastError = e.getMessage();
            return "{}";
        }
    }

    private native String nativeToJson(Object instance);

    public boolean validate() {
        if (wasmInstance == null) {
            return false;
        }
        try {
            return nativeValidate(wasmInstance);
        } catch (Exception e) {
            return false;
        }
    }

    private native boolean nativeValidate(Object instance);

    public String getLastError() {
        return lastError;
    }

    @Override
    public void close() {
        if (wasmInstance != null) {
            try {
                nativeClose(wasmInstance);
            } catch (Exception e) {
                // Ignore close errors
            }
            wasmInstance = null;
        }
    }

    private native void nativeClose(Object instance);

    public static String getVersion() {
        return "0.1.0";
    }

    public static class VerseConfException extends Exception {
        public VerseConfException(String message) {
            super(message);
        }

        public VerseConfException(String message, Throwable cause) {
            super(message, cause);
        }
    }
}
