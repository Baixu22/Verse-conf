use criterion::{black_box, criterion_group, criterion_main, Criterion};
use verseconf_core::{parse, parse_and_validate, format, PrettyPrinter, PrettyPrintConfig};

fn generate_large_config(size: usize) -> String {
    let mut config = String::new();
    
    for i in 0..size {
        config.push_str(&format!("key_{} = \"value_{}\"\n", i, i));
    }
    
    config.push_str("\nserver {\n");
    for i in 0..size / 10 {
        config.push_str(&format!("    port_{} = {}\n", i, 8000 + i));
    }
    config.push_str("}\n");
    
    config.push_str("\nitems = [\n");
    for i in 0..size / 5 {
        config.push_str(&format!("    {},\n", i));
    }
    config.push_str("]\n");
    
    config
}

fn bench_parse_small(c: &mut Criterion) {
    let source = r#"
name = "test"
version = "1.0.0"
port = 8080
debug = true
timeout = 30s

server {
    host = "localhost"
    port = 3000
}
"#;
    c.bench_function("parse_small", |b| b.iter(|| parse(black_box(source))));
}

fn bench_parse_medium(c: &mut Criterion) {
    let source = generate_large_config(100);
    c.bench_function("parse_medium_100_keys", |b| b.iter(|| parse(black_box(&source))));
}

fn bench_parse_large(c: &mut Criterion) {
    let source = generate_large_config(1000);
    c.bench_function("parse_large_1000_keys", |b| b.iter(|| parse(black_box(&source))));
}

fn bench_parse_xlarge(c: &mut Criterion) {
    let source = generate_large_config(5000);
    c.bench_function("parse_xlarge_5000_keys", |b| b.iter(|| parse(black_box(&source))));
}

fn bench_validate_small(c: &mut Criterion) {
    let source = r#"
name = "test"
port = 8080 #@ range(1..65535)
timeout = 30s
"#;
    c.bench_function("validate_small", |b| b.iter(|| parse_and_validate(black_box(source))));
}

fn bench_format_small(c: &mut Criterion) {
    let source = r#"
name="test"
port=8080
server{host="localhost",port=3000}
"#;
    c.bench_function("format_small", |b| b.iter(|| format(black_box(source))));
}

fn bench_pretty_print_custom(c: &mut Criterion) {
    let source = generate_large_config(100);
    let ast = parse(&source).unwrap();
    
    let config = PrettyPrintConfig {
        indent_size: 4,
        inline_short_arrays: false,
        ..Default::default()
    };
    
    c.bench_function("pretty_print_custom_indent", |b| {
        b.iter(|| PrettyPrinter::print_with_config(black_box(&ast), black_box(config.clone())))
    });
}

criterion_group!(
    benches,
    bench_parse_small,
    bench_parse_medium,
    bench_parse_large,
    bench_parse_xlarge,
    bench_validate_small,
    bench_format_small,
    bench_pretty_print_custom,
);
criterion_main!(benches);
