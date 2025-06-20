# Spine

A modern replacement for `npm link` with interactive configuration management for local package development.

## Features

- Interactive TUI with arrow key navigation
- TOML-based configuration storage
- Package.json version awareness
- **Automatic npm linking** with `link-all` and `link` commands
- **Project status tracking** to see what's currently linked
- **Easy unlinking** with `unlink` command
- Fast, memory-safe Rust implementation
- Command-line and interactive modes

## Installation

```bash
cargo build --release
```

## Usage

### Interactive Mode (Default)
```bash
cargo run
# or
cargo run -- interactive
```

### Command Line Mode

#### Configuration Management
```bash
# List current links
cargo run -- list

# Add a new link
cargo run -- add my-package /path/to/local/package
cargo run -- add "@scope/package" ~/projects/scoped-package

# Remove a link
cargo run -- remove my-package
```

#### NPM Linking Operations
```bash
# Link all configured packages to current project
cargo run -- link-all

# Link specific package to current project  
cargo run -- link my-package
cargo run -- link "@scope/package"

# Show linking status for current project
cargo run -- status

# Unlink specific package from current project
cargo run -- unlink my-package
```

### Interactive Controls

- **Arrow Keys** or **j/k**: Navigate up/down
- **a**: Add new package link
- **r** or **Delete**: Remove selected package link
- **h**: Show help
- **q** or **Esc**: Quit

## Configuration

Configuration is stored in `~/.config/spine/config.toml` and automatically created on first run.

Example configuration:
```toml
[links.my-package]
name = "my-package"
path = "/Users/username/projects/my-package"
version = "1.0.0"
```

## Typical Workflow

### 1. **Configure Your Local Packages**
```bash
# Add your locally developed packages to Spine
cargo run -- add "@mycompany/ui-lib" ~/projects/ui-library/dist
cargo run -- add "@mycompany/utils" ~/projects/shared-utils/dist
cargo run -- add "my-local-package" ~/dev/my-package

# View configured packages
cargo run -- list
```

### 2. **Work on a Project**
```bash
# Navigate to your project directory
cd ~/projects/my-app

# Link all your local packages at once
cargo run -- link-all

# Or link specific packages
cargo run -- link "@mycompany/ui-lib"
cargo run -- link "my-local-package"

# Check what's linked in this project
cargo run -- status
```

### 3. **Unlink When Done**
```bash
# Unlink specific packages
cargo run -- unlink "@mycompany/ui-lib"

# Or manually unlink all and reinstall from npm
npm unlink && npm install
```

## Configuration

Configuration is stored in `~/.config/spine/config.toml` and automatically created on first run.

Example configuration:
```toml
[links.my-package]
name = "my-package"
path = "/Users/username/projects/my-package"
version = "1.0.0"

[links."@mycompany/ui-lib"]
name = "@mycompany/ui-lib"
path = "/Users/username/projects/ui-library/dist"
version = "2.1.0"
```

## Requirements

- Rust 1.70+
- Node.js and npm
- Valid package.json files in linked directories