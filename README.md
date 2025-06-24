# ClearModel - Secure ML Cache Cleaner

A secure, high-performance Rust application for cleaning machine learning model caches with comprehensive path traversal protection and modern async architecture.

## Features

- 🛡️ **Security First**: Comprehensive path traversal protection following [2025 security best practices](https://www.stackhawk.com/blog/rust-path-traversal-guide-example-and-prevention/)
- ⚡ **High Performance**: Async operations with configurable concurrency limits
- 🎯 **ML Framework Support**: Cleans caches for HuggingFace, PyTorch, TensorFlow, Keras, and more
- 🔧 **Configurable**: TOML/YAML/JSON configuration with environment variable overrides
- 📊 **Comprehensive Logging**: Structured logging with operation statistics
- 🧪 **Dry Run Mode**: Preview what would be cleaned without making changes
- 🏗️ **Modular Architecture**: Clean separation of concerns with proper error handling

## Installation

### From Source

```bash
git clone <repository-url>
cd clearmodel
cargo build --release
```

The binary will be available at `target/release/clearmodel`.

### Dependencies

The application uses modern Rust 2025 libraries:
- `tokio` - Async runtime
- `tracing` - Structured logging
- `camino` - UTF-8 paths
- `path-clean` - Path normalization
- `sanitize-filename` - Filename sanitization
- `walkdir` - Safe directory traversal
- `secrecy` - Secure secret handling
- `sysinfo` - System information

## Quick Start

1. **Set up environment**:
   ```bash
   # Run clearmodel - it will create a default .env file
   ./clearmodel
   ```

2. **Configure environment** (optional):
   Edit `.env` to customize settings:
   ```bash
   # Password for sudo operations (will be prompted if not provided)
   SUDO_PASSWORD=
   
   # Enable debug mode
   DEBUG=false
   
   # Maximum number of parallel cache operations
   MAX_PARALLEL_OPERATIONS=10
   ```

3. **Run cache cleanup**:
   ```bash
   # Dry run to see what would be cleaned
   ./clearmodel --dry-run --verbose
   
   # Actually clean the caches
   ./clearmodel --verbose
   ```

## Configuration

### Environment Variables

The application supports these environment variables:

| Variable | Required | Description | Default |
|----------|----------|-------------|---------|
| `SUDO_PASSWORD` | No | Password for sudo operations (prompted if needed) | - |
| `DEBUG` | No | Enable debug mode | `false` |
| `LOG_LEVEL` | No | Logging level | `INFO` |
| `MAX_PARALLEL_OPERATIONS` | No | Max parallel operations | `10` |
| `CACHE_RETENTION_DAYS` | No | Days to retain cache files | `7` |

### Configuration File

Create `clearmodel.toml` in your working directory:

```toml
# Cache directories to clean
cache_paths = [
    "~/.cache/huggingface",
    "~/.cache/torch", 
    "~/.cache/tensorflow",
    "~/.cache/transformers",
    "~/Library/Caches/torch"  # macOS
]

# Maximum age of cache files in days
max_cache_age_days = 7

# Maximum number of parallel operations
max_parallel_operations = 10

# Whether to follow symbolic links
follow_symlinks = false

# File extensions to target for Python cache cleanup
python_cache_extensions = [".pyc", ".pyo", ".pyd"]

# Directories to skip during cleanup
skip_directories = [
    ".git", ".svn", "node_modules", 
    ".venv", "venv", "__pycache__"
]

# Minimum free space threshold (in GB) before cleanup
min_free_space_gb = 1

# Security settings
[security]
validate_cache_paths = true
check_path_traversal = true
max_path_depth = 20
require_confirmation_threshold_gb = 10
```

## Security Features

### Path Traversal Protection

The application implements multiple layers of security to prevent path traversal attacks:

1. **Path Normalization**: Uses `path-clean` to resolve `..` and `.` components
2. **Boundary Validation**: Ensures paths don't escape allowed directories
3. **Component Validation**: Checks individual path components for suspicious patterns
4. **UTF-8 Compliance**: Uses `camino` for cross-platform UTF-8 path handling
5. **System Path Protection**: Prevents deletion of critical system directories

### Example Security Checks

```rust
// These paths would be rejected:
"../../../etc/passwd"           // Path traversal attempt
"cache/../../../home/user"      // Relative path escape
"/System/Library"               // Critical system path (macOS)
"Documents/important.txt"       // User data directory
```

### Secure Secret Handling

- Passwords stored using `secrecy` crate with secure prompting via `rpassword`
- Memory is zeroed on drop automatically
- No secrets in logs or error messages
- Interactive password prompting when sudo access is needed
- Optional environment variable storage for automation

## Command Line Usage

```bash
clearmodel [OPTIONS]

OPTIONS:
    -d, --debug              Enable debug logging
    -c, --config <FILE>      Configuration file path
    -n, --dry-run           Show what would be cleaned without cleaning
    -v, --verbose           Verbose output
    -h, --help              Print help information
    -V, --version           Print version information
```

### Examples

```bash
# Dry run with verbose output
clearmodel --dry-run --verbose

# Use custom configuration file
clearmodel --config /path/to/config.toml

# Debug mode
clearmodel --debug

# Estimate cleanup space
clearmodel --dry-run | grep "Estimated cleanup space"
```

## Supported Cache Types

### Machine Learning Frameworks

- **HuggingFace**: `~/.cache/huggingface/`, uses `huggingface-cli delete-cache` if available
- **PyTorch**: `~/.cache/torch/`, `~/Library/Caches/torch/` (macOS)
- **TensorFlow**: `~/.cache/tensorflow/`
- **Keras**: `~/.cache/keras/`, `~/.keras/`
- **Transformers**: `~/.cache/transformers/`, `~/.transformers/`
- **OpenAI**: `~/.cache/openai/`
- **Anthropic**: `~/.cache/anthropic/`

### Python Cache Files

- `.pyc` files (compiled Python)
- `.pyo` files (optimized Python)
- `.pyd` files (Python extension modules)
- `__pycache__/` directories

## Performance

### Async Architecture

- Non-blocking I/O operations
- Configurable concurrency limits
- Batch processing for large directories
- Resource usage monitoring

### Benchmarks

On a MacBook Pro M4 Max:
- ~10,000 files/second processing rate
- Memory usage: <50MB typical
- CPU usage: Scales with configured parallelism

## Error Handling

The application uses structured error handling with detailed context:

```rust
pub enum ClearModelError {
    Configuration { message: String },
    PathTraversal { path: PathBuf },
    FileOperation { message: String, path: Option<PathBuf> },
    Security { message: String },
    // ... more error types
}
```

All errors include:
- Detailed error messages
- File paths when relevant
- Suggestions for resolution
- No sensitive information leakage

## Development

### Building

```bash
# Debug build
cargo build

# Release build (optimized)
cargo build --release

# Run tests
cargo test

# Run with logging
RUST_LOG=debug cargo run -- --dry-run
```

### Testing

```bash
# Run all tests
cargo test

# Run tests with output
cargo test -- --nocapture

# Run specific test
cargo test test_path_traversal_prevention
```

### Contributing

1. Follow Rust best practices
2. Add tests for new functionality
3. Update documentation
4. Ensure security measures are maintained

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## Security Reporting

If you discover a security vulnerability, please report it responsibly:
- Do not create public issues
- Contact the maintainers directly
- Provide detailed reproduction steps

## Acknowledgments

- [StackHawk](https://www.stackhawk.com/blog/rust-path-traversal-guide-example-and-prevention/) for path traversal security guidance
- Rust community for excellent crates and documentation
- ML framework communities for cache management insights 