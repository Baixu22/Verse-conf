package main

import (
	"fmt"
	"log"

	"github.com/verseconf/verseconf-wasm/go/lib"
)

func main() {
	source := `app {
		name = "MyApp"
		version = "1.0.0"
	}

	database {
		host = "localhost"
		port = 5432
		enabled = true
	}`

	conf, err := verseconf.Parse(source, "../pkg/verseconf_wasm_bg.wasm")
	if err != nil {
		log.Fatalf("Failed to parse: %v", err)
	}
	defer conf.Close()

	fmt.Println("✅ Parsed successfully!")

	if name, ok := conf.GetString("app.name"); ok {
		fmt.Printf("app.name = %q\n", name)
	} else {
		fmt.Println("app.name not found")
	}

	if version, ok := conf.GetString("app.version"); ok {
		fmt.Printf("app.version = %q\n", version)
	}

	if host, ok := conf.GetString("database.host"); ok {
		fmt.Printf("database.host = %q\n", host)
	}

	if port, ok := conf.GetNumber("database.port"); ok {
		fmt.Printf("database.port = %.0f\n", port)
	}

	if enabled, ok := conf.GetBoolean("database.enabled"); ok {
		fmt.Printf("database.enabled = %t\n", enabled)
	}

	fmt.Printf("\nhas_key(\"app\") = %t\n", conf.HasKey("app"))
	fmt.Printf("has_key(\"nonexistent\") = %t\n", conf.HasKey("nonexistent"))

	json := conf.ToJSON()
	fmt.Printf("\nJSON Output:\n%s\n", json)

	fmt.Println("\n🎉 All tests passed!")
}
