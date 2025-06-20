# Spine

[![Rust](https://img.shields.io/badge/rust-1.70+-blue.svg)](https://www.rust-lang.org/)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

A powerful, modern replacement for `npm link` with interactive configuration management, live status monitoring, and Angular workspace integration. Built with Rust for speed, safety, and reliability.

## ✨ Features

### Core Functionality
- **🔗 Smart Package Linking** - Advanced npm link management with verification
- **📊 Live Status Monitoring** - Real-time health checks and link status
- **🎨 Interactive TUI** - Rich terminal interface with keyboard navigation
- **⚡ Fast CLI Interface** - Comprehensive command-line tools with aliases
- **🔧 TOML Configuration** - Human-readable configuration storage

### Advanced Features  
- **🅰️ Angular Integration** - Build, serve, and test Angular libraries seamlessly
- **🔍 Package Discovery** - Automatic workspace scanning and package detection
- **🩺 Health Monitoring** - Broken link detection and package validation
- **🔄 Auto-Sync** - Restore links after `npm install` or system changes
- **🚀 Shell Completion** - Auto-generated completions for bash/zsh/fish/powershell
- **📄 JSON Output** - Scriptable output for CI/CD pipelines
- **🎯 Fuzzy Matching** - Smart suggestions for mistyped package names

## 🚀 Installation

### Prerequisites
- Rust 1.70+ 
- Node.js and npm
- Angular CLI (optional, for Angular features)

### Build from Source
```bash
git clone <repository-url>
cd spine
cargo build --release
```

The binary will be available at `target/release/spine`.

## 📖 Usage

Spine offers multiple interfaces: **Interactive TUI** (default), **CLI commands**, and **command aliases** for power users.

### 🎮 Interactive Mode (Recommended)

Launch the interactive interface with live status monitoring:

```bash
spine
# or explicitly
spine interactive
```

#### Interactive Controls
- **↑/↓ or j/k** - Navigate packages
- **a** - Add new package link  
- **r/Delete** - Remove selected package
- **l** - Link package to current project
- **u** - Unlink package from current project
- **b** - Build Angular library (if detected)
- **t** - Test Angular library (if detected)
- **h** - Show help
- **F5** - Refresh status
- **q/Esc** - Quit

#### Status Indicators
- **✅** - Package healthy
- **⚠️** - Warning (e.g., missing dependencies)
- **❌** - Broken (e.g., invalid path)
- **🔗** - Currently linked
- **🔓** - Not linked
- **🅰️** - Angular library detected

### 💻 CLI Commands

#### Package Management
```bash
# Add packages (auto-detects name from package.json)
spine add                                    # Current directory
spine add my-package                         # Specify name
spine add my-package /path/to/package        # Specify name and path
spine add "@scope/package" ~/projects/lib    # Scoped packages

# List configured packages
spine list                                   # or: spine l

# Remove packages  
spine remove my-package

# Scan workspace for packages
spine scan                                   # Discovery mode
spine scan --add                             # Auto-add discovered packages
spine scan --path ~/projects                 # Scan specific directory
```

#### Link Management
```bash
# Link operations
spine link-all                               # Link all configured packages
spine link my-package                        # Link specific package
spine unlink my-package                      # Unlink specific package
spine unlink-all                             # Unlink all packages

# Status and health
spine status                                 # Basic status
spine status --detailed                      # Detailed information
spine status --health                        # Health check
spine status --json                          # JSON output for scripts

# Maintenance
spine verify                                 # Clean up broken links
spine sync                                   # Restore links per configuration
```

#### Angular Integration
```bash
# Build libraries
spine build                                  # Build all libraries
spine build my-lib                           # Build specific library
spine build --all                            # Build all linked libraries
spine build --watch                          # Watch mode
spine build --affected                       # Build only affected

# Development server
spine serve                                  # Standard serve
spine serve --with-libs                      # Auto-rebuild libraries
spine serve --port 4200 --hmr               # Custom port with HMR
spine serve my-app                           # Serve specific project

# Angular CLI integration
spine ng generate component my-comp --lib my-lib
spine ng-proxy build --prod                 # Proxy any ng command

# Publishing
spine publish my-package                     # Build and publish
spine publish my-package --skip-build        # Publish without building
spine publish my-package --dry-run           # Test publish
```

#### Power User Aliases
```bash
spine s --with-libs                          # Alias for serve
spine l                                      # Alias for list  
spine a my-package                           # Alias for add
spine g component my-comp --lib my-lib       # Alias for ng generate
```

### 🔧 Configuration

Spine stores configuration in `~/.config/spine/config.toml` (created automatically).

#### Example Configuration
```toml
[links."@company/ui-lib"]
name = "@company/ui-lib"
path = "/Users/dev/projects/ui-library/dist"
version = "2.1.0"
linked_projects = [
    "/Users/dev/projects/main-app",
    "/Users/dev/projects/admin-app"
]

[links."utils-package"]
name = "utils-package"  
path = "/Users/dev/projects/shared-utils"
version = "1.0.0"
linked_projects = []

[completion]
auto_regenerate = true
shell = "zsh"
script_path = "/Users/dev/.spine_completion.zsh"
```

#### Advanced Configuration

```bash
# Configuration management
spine config-edit                           # Open config in editor

# Shell completion
spine generate-completion zsh                # Generate completion script
spine enable-auto-completion                 # Enable auto-regeneration
spine disable-auto-completion                # Disable auto-regeneration

# Debug Angular workspace
spine debug --workspace                      # Show workspace info
spine debug --libs                           # Show library detection
```

## 🎯 Workflows

### 📦 Initial Setup

1. **Configure your local packages:**
```bash
cd ~/projects/my-ui-library
spine add                                    # Auto-detects package name

cd ~/projects/shared-utils  
spine add utils-package                      # Specify custom name

spine list                                   # Verify configuration
```

2. **Set up shell completion (optional):**
```bash
spine enable-auto-completion                 # Auto-detects shell
# Follow instructions to add to shell config
```

### 🔄 Daily Development

1. **Start working on a project:**
```bash
cd ~/projects/my-app
spine link-all                               # Link all configured packages
spine status                                 # Verify links
```

2. **Interactive development (recommended):**
```bash
spine                                        # Launch interactive mode
# Use 'l' to link packages, 'b' to build libraries, etc.
```

3. **With Angular libraries:**
```bash
spine serve --with-libs                      # Auto-rebuilding dev server
# In another terminal:
spine build --watch my-lib                   # Watch specific library
```

### 🧹 Maintenance

```bash
spine verify                                 # Check for broken links
spine sync                                   # Restore links after npm install
spine status --health                        # Comprehensive health check
```

### 🚀 CI/CD Integration

```bash
# In CI scripts
#!/bin/bash
spine status --json > link-status.json      # Export status
spine verify                                 # Cleanup broken links
spine sync                                   # Ensure consistency
```

## 🎨 Angular Workspace Integration

Spine provides first-class Angular support with automatic workspace detection:

### Features
- **📁 Workspace Detection** - Automatically detects `angular.json`
- **🏗️ Library Building** - Build libraries with dependency tracking
- **🔄 Hot Reloading** - Auto-rebuild on library changes during serve
- **🧪 Testing Integration** - Run tests on specific libraries
- **📊 Project Mapping** - Maps Spine packages to Angular projects

### Angular-Specific Commands
```bash
# Automatic workspace detection
spine                                        # TUI shows Angular context

# Library development
spine build my-lib                           # Build specific library
spine build --all --watch                   # Build all with watch
spine test my-lib                            # Run library tests

# Development server with library rebuild
spine serve --with-libs                      # Serve with auto-library rebuild
spine s --with-libs --port 4200              # Alias with custom port

# Code generation
spine g component my-component --lib my-lib  # Generate in library
spine ng generate service my-service         # Standard ng generate
```

## 🔍 Status Monitoring

Spine continuously monitors package health and link status:

### Health Checks
- **Package existence** - Verifies paths exist
- **package.json validity** - Ensures valid package metadata  
- **Symlink integrity** - Detects broken symlinks
- **Version tracking** - Monitors version changes
- **Dependency validation** - Checks for missing dependencies

### Status Outputs
```bash
spine status                                 # Human-readable summary
spine status --detailed                      # Verbose information
spine status --health                        # Health-focused report
spine status --json                          # Machine-readable output
```

Example detailed status:
```
Package Links (3📦 | 2🔗 | 2✅ | 1⚠️ | 0❌)
✅ 🔗 @company/ui-lib (v2.1.0) 🅰️ -> /Users/dev/ui-library/dist
    └─ 🔗 Linked to: /Users/dev/main-app
⚠️ 🔓 utils-package (v1.0.0) -> /Users/dev/shared-utils
    └─ ⚠️ Missing peer dependency: lodash
✅ 🔓 my-library (v0.1.0) 🅰️ -> /Users/dev/my-lib/dist
```

## 🛠️ Advanced Features

### Shell Completion
```bash
# Enable auto-completion (detects shell automatically)
spine enable-auto-completion

# Manual completion generation
spine generate-completion bash > ~/.spine_completion.bash
echo 'source ~/.spine_completion.bash' >> ~/.bashrc

# For zsh users
spine generate-completion zsh > ~/.spine_completion.zsh
echo 'source ~/.spine_completion.zsh' >> ~/.zshrc
```

### Workspace Scanning
Create a `.spine.toml` in your workspace root to configure auto-discovery:

```toml
[workspace]
# Patterns to include/exclude during scanning
include_patterns = ["**/dist", "**/build", "**/lib"]
exclude_patterns = ["**/node_modules", "**/*.test.*"]

# Auto-link patterns
auto_link = true
link_to_projects = ["./apps/*/"]
```

## 🔧 Troubleshooting

### Common Issues

**Links not working after `npm install`:**
```bash
spine sync                                   # Restore configured links
```

**Package not found:**
```bash
spine list                                   # Check configured packages
spine scan --add                             # Auto-discover packages
```

**Broken symlinks:**
```bash
spine verify                                 # Clean up broken links
spine status --health                        # Detailed health report
```

**Angular workspace not detected:**
```bash
spine debug --workspace                      # Debug workspace detection
# Ensure you're in project root with angular.json
```

### Debug Commands
```bash
spine debug --workspace                      # Angular workspace info
spine debug --libs                           # Library detection details
spine list-packages-for-completion           # Available packages for completion
```

## 📊 JSON API

For integration with other tools, Spine provides JSON output:

```bash
spine status --json
```

Example output:
```json
{
  "packages": [
    {
      "name": "@company/ui-lib",
      "version": "2.1.0", 
      "path": "/Users/dev/ui-library/dist",
      "health": "healthy",
      "linked": true,
      "is_angular_lib": true,
      "linked_projects": ["/Users/dev/main-app"]
    }
  ],
  "summary": {
    "total_packages": 3,
    "linked_count": 2,
    "healthy_count": 2,
    "warning_count": 1,
    "broken_count": 0
  }
}
```

## 🏗️ Architecture

Spine is built with:
- **Rust** - Memory safety and performance
- **Ratatui** - Rich terminal user interface
- **Clap** - Command-line argument parsing
- **Serde/TOML** - Configuration serialization
- **Crossterm** - Cross-platform terminal handling

The architecture is modular with separate concerns for:
- Configuration management (`config.rs`)
- TUI interface (`tui.rs`) 
- CLI parsing (`cli.rs`)
- NPM operations (`npm.rs`)
- Angular integration (`angular.rs`)
- Package scanning (`scanner.rs`)
- Error handling with suggestions (`error.rs`)

## 🤝 Contributing

Contributions are welcome! Please feel free to submit issues and pull requests.

## 📄 License

This project is licensed under the MIT License - see the LICENSE file for details.

---

**Spine** - Making local package development a breeze! 🌊