package com.verseconf;

import java.io.*;
import java.net.HttpURLConnection;
import java.net.URL;
import java.nio.file.*;

public class VerseConfTest {
    private static final String WASM_WASM_PATH = "../../../integrations/verseconf-wasm/pkg/verseconf_wasm_bg.wasm";

    public static void main(String[] args) {
        System.out.println("[TEST] VerseConf Java WASM Test");
        System.out.println("============================\n");

        String source = "app {\n" +
                "    name = \"MyApp\"\n" +
                "    version = \"1.0.0\"\n" +
                "}\n" +
                "\n" +
                "database {\n" +
                "    host = \"localhost\"\n" +
                "    port = 5432\n" +
                "    enabled = true\n" +
                "}";

        System.out.println("Input VCF:");
        System.out.println(source);
        System.out.println();

        try {
            Path wasmPath = Paths.get(WASM_WASM_PATH);
            if (!Files.exists(wasmPath)) {
                System.out.println("[WARN] WASM file not found at: " + wasmPath.toAbsolutePath());
                System.out.println("       Using HTTP server test instead...\n");
                testViaHttpServer(source);
            } else {
                System.out.println("[OK] WASM file found: " + wasmPath.toAbsolutePath());
                testWithWasmtime(source);
            }
        } catch (Exception e) {
            System.out.println("[ERROR] " + e.getMessage());
            e.printStackTrace();
        }
    }

    private static void testViaHttpServer(String source) {
        try {
            URL url = new URL("http://localhost:8080/test.html");
            HttpURLConnection conn = (HttpURLConnection) url.openConnection();
            conn.setRequestMethod("GET");

            int responseCode = conn.getResponseCode();
            if (responseCode == 200) {
                System.out.println("[OK] HTTP server is running at localhost:8080");
                System.out.println("     Please open http://localhost:8080/test.html in a browser to test WASM");
            } else {
                System.out.println("[WARN] HTTP server not responding (code: " + responseCode + ")");
                System.out.println("       Start server with: python -m http.server 8080");
            }
        } catch (Exception e) {
            System.out.println("[WARN] Cannot connect to HTTP server: " + e.getMessage());
            System.out.println("       Start server with: python -m http.server 8080");
        }
    }

    private static void testWithWasmtime(String source) {
        System.out.println("[INFO] Direct WASMtime execution requires JNI native library");
        System.out.println("       The Java WASM binding requires compilation of C/C++ JNI code");
        System.out.println();
        System.out.println("       To fully test Java WASM bindings:");
        System.out.println("       1. Install wasmtime-java or compile JNI library");
        System.out.println("       2. Or use the browser test at http://localhost:8080/test.html");
    }
}
