use std::io;
use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::{Backend, CrosstermBackend},
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap},
    Frame, Terminal,
};
use crate::config::{Config, PackageLink};
use crate::npm::NpmManager;
use crate::angular::AngularBuildManager;
use std::time::{Instant, Duration};
use std::collections::HashMap;

pub struct TuiApp {
    config: Config,
    selected_index: usize,
    mode: AppMode,
    input_buffer: String,
    add_mode_field: AddModeField,
    show_help: bool,
    workspace_root: std::path::PathBuf,
    package_status: HashMap<String, PackageStatus>,
    angular_workspace: Option<crate::angular::AngularWorkspace>,
    last_refresh: Instant,
    current_project_path: std::path::PathBuf,
}

#[derive(Debug, Clone)]
pub struct PackageStatus {
    pub health: HealthStatus,
    pub link_status: LinkStatus,
    pub is_angular_lib: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum HealthStatus {
    Healthy,
    Warning(String),
    Broken(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum LinkStatus {
    Linked,
    Unlinked,
    Unknown,
}

#[derive(Debug, Clone, PartialEq)]
enum AppMode {
    Normal,
    AddPackage,
    RemovePackage,
    Help,
    LinkPackage,
    UnlinkPackage,
    BuildPackage,
    TestPackage,
}

#[derive(Debug, Clone, PartialEq)]
enum AddModeField {
    Name,
    Path,
}

impl TuiApp {
    pub fn new(config: Config) -> Result<Self> {
        let workspace_root = std::env::current_dir()?;
        let current_project_path = workspace_root.clone();
        let angular_workspace = AngularBuildManager::detect_angular_workspace(&workspace_root).ok().flatten();
        
        let mut app = Self {
            config,
            selected_index: 0,
            mode: AppMode::Normal,
            input_buffer: String::new(),
            add_mode_field: AddModeField::Name,
            show_help: false,
            workspace_root,
            package_status: HashMap::new(),
            angular_workspace,
            last_refresh: Instant::now(),
            current_project_path,
        };
        
        app.refresh_package_status()?;
        Ok(app)
    }

    fn refresh_package_status(&mut self) -> Result<()> {
        for (package_name, package_link) in &self.config.links {
            let health = self.check_package_health(package_link);
            let link_status = self.check_link_status(package_name);
            let is_angular_lib = self.is_angular_library(package_link);

            self.package_status.insert(package_name.clone(), PackageStatus {
                health,
                link_status,
                is_angular_lib,
            });
        }
        self.last_refresh = Instant::now();
        Ok(())
    }

    fn check_package_health(&self, package_link: &PackageLink) -> HealthStatus {
        // Check if path exists
        if !package_link.path.exists() {
            return HealthStatus::Broken("Path does not exist".to_string());
        }

        // Check if package.json exists
        let package_json_path = package_link.path.join("package.json");
        if !package_json_path.exists() {
            return HealthStatus::Broken("No package.json found".to_string());
        }

        // Try to parse package.json
        if let Err(_) = crate::package::parse_package_json(&package_json_path) {
            return HealthStatus::Broken("Invalid package.json".to_string());
        }

        // Check for symlink issues
        if package_link.path.is_symlink() {
            if let Err(_) = package_link.path.read_link() {
                return HealthStatus::Warning("Broken symlink".to_string());
            }
        }

        HealthStatus::Healthy
    }

    fn check_link_status(&self, package_name: &str) -> LinkStatus {
        let node_modules_path = self.current_project_path.join("node_modules");
        if !node_modules_path.exists() {
            return LinkStatus::Unlinked;
        }
        
        let package_path = if package_name.starts_with('@') {
            let parts: Vec<&str> = package_name.splitn(2, '/').collect();
            if parts.len() == 2 {
                node_modules_path.join(parts[0]).join(parts[1])
            } else {
                node_modules_path.join(package_name)
            }
        } else {
            node_modules_path.join(package_name)
        };
        
        if package_path.is_symlink() {
            // Verify the symlink target exists and is valid
            if package_path.read_link().is_ok() && package_path.exists() {
                LinkStatus::Linked
            } else {
                LinkStatus::Unknown // Broken symlink
            }
        } else if package_path.exists() {
            LinkStatus::Unlinked // Regular directory/file, not linked
        } else {
            LinkStatus::Unlinked
        }
    }


    fn is_angular_library(&self, package_link: &PackageLink) -> bool {
        // Check if this is an Angular library by looking for Angular-specific files
        package_link.path.join("ng-package.json").exists() ||
        package_link.path.join("public-api.ts").exists() ||
        (self.angular_workspace.is_some() && 
         package_link.path.to_string_lossy().contains("dist"))
    }

    fn get_total_items(&self) -> usize {
        let mut count = 0;
        
        // Sort packages alphabetically by name (same as display order)
        let mut sorted_links: Vec<_> = self.config.links.values().collect();
        sorted_links.sort_by(|a, b| a.name.cmp(&b.name));
        
        for link in sorted_links {
            count += 1; // Package itself
            
            // Count health details if there are issues
            if let Some(status) = self.package_status.get(&link.name) {
                if let HealthStatus::Warning(_) | HealthStatus::Broken(_) = &status.health {
                    count += 1; // Health detail line
                }
            }
            
            // Count linked projects
            count += link.linked_projects.len();
        }
        count
    }

    fn get_package_at_index(&self, target_index: usize) -> Option<String> {
        let mut current_index = 0;
        
        // Sort packages alphabetically by name (same as display order)
        let mut sorted_links: Vec<_> = self.config.links.values().collect();
        sorted_links.sort_by(|a, b| a.name.cmp(&b.name));
        
        for link in sorted_links {
            if current_index == target_index {
                return Some(link.name.clone());
            }
            current_index += 1;
            
            // Skip health details if there are issues
            if let Some(status) = self.package_status.get(&link.name) {
                if let HealthStatus::Warning(_) | HealthStatus::Broken(_) = &status.health {
                    if current_index == target_index {
                        return Some(link.name.clone()); // Return parent package name
                    }
                    current_index += 1;
                }
            }
            
            // Skip linked projects
            for _ in &link.linked_projects {
                if current_index == target_index {
                    return Some(link.name.clone()); // Return parent package name
                }
                current_index += 1;
            }
        }
        None
    }

    pub fn run(&mut self) -> Result<()> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        let result = self.run_app(&mut terminal);

        disable_raw_mode()?;
        execute!(
            terminal.backend_mut(),
            LeaveAlternateScreen,
            DisableMouseCapture
        )?;
        terminal.show_cursor()?;

        result
    }

    fn run_app<B: Backend>(&mut self, terminal: &mut Terminal<B>) -> Result<()> {
        loop {
            // Auto-refresh package status every 5 seconds
            if self.last_refresh.elapsed() > Duration::from_secs(5) {
                let _ = self.refresh_package_status();
            }

            terminal.draw(|f| self.ui(f))?;

            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match self.mode {
                        AppMode::Normal => {
                            if self.handle_normal_mode_input(key.code)? {
                                break;
                            }
                        }
                        AppMode::AddPackage => {
                            if self.handle_add_mode_input(key.code)? {
                                self.mode = AppMode::Normal;
                            }
                        }
                        AppMode::RemovePackage => {
                            if self.handle_remove_mode_input(key.code)? {
                                self.mode = AppMode::Normal;
                            }
                        }
                        AppMode::LinkPackage => {
                            if self.handle_link_mode_input(key.code)? {
                                self.mode = AppMode::Normal;
                                let _ = self.refresh_package_status();
                            }
                        }
                        AppMode::UnlinkPackage => {
                            if self.handle_unlink_mode_input(key.code)? {
                                self.mode = AppMode::Normal;
                                let _ = self.refresh_package_status();
                            }
                        }
                        AppMode::BuildPackage => {
                            if self.handle_build_mode_input(key.code)? {
                                self.mode = AppMode::Normal;
                                let _ = self.refresh_package_status();
                            }
                        }
                        AppMode::TestPackage => {
                            if self.handle_test_mode_input(key.code)? {
                                self.mode = AppMode::Normal;
                                let _ = self.refresh_package_status();
                            }
                        }
                        AppMode::Help => {
                            if matches!(key.code, KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('h')) {
                                self.mode = AppMode::Normal;
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn handle_normal_mode_input(&mut self, key: KeyCode) -> Result<bool> {
        match key {
            KeyCode::Char('q') | KeyCode::Esc => return Ok(true),
            KeyCode::Char('h') => self.mode = AppMode::Help,
            KeyCode::Char('a') => {
                self.mode = AppMode::AddPackage;
                self.input_buffer.clear();
                self.add_mode_field = AddModeField::Name;
            }
            KeyCode::Char('r') | KeyCode::Delete => {
                if !self.config.links.is_empty() {
                    self.mode = AppMode::RemovePackage;
                }
            }
            KeyCode::Char('l') => {
                if !self.config.links.is_empty() {
                    self.mode = AppMode::LinkPackage;
                }
            }
            KeyCode::Char('u') => {
                if !self.config.links.is_empty() {
                    self.mode = AppMode::UnlinkPackage;
                }
            }
            KeyCode::Char('b') => {
                if !self.config.links.is_empty() && self.angular_workspace.is_some() {
                    self.mode = AppMode::BuildPackage;
                }
            }
            KeyCode::Char('t') => {
                if !self.config.links.is_empty() && self.angular_workspace.is_some() {
                    self.mode = AppMode::TestPackage;
                }
            }
            KeyCode::F(5) => {
                // F5 to refresh
                let _ = self.refresh_package_status();
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if self.selected_index > 0 {
                    self.selected_index -= 1;
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.selected_index < self.get_total_items().saturating_sub(1) {
                    self.selected_index += 1;
                }
            }
            _ => {}
        }
        Ok(false)
    }

    fn handle_add_mode_input(&mut self, key: KeyCode) -> Result<bool> {
        match key {
            KeyCode::Esc => return Ok(true),
            KeyCode::Enter => {
                match self.add_mode_field {
                    AddModeField::Name => {
                        if !self.input_buffer.trim().is_empty() {
                            self.add_mode_field = AddModeField::Path;
                            self.input_buffer.push('\n');
                        }
                    }
                    AddModeField::Path => {
                        let parts: Vec<&str> = self.input_buffer.split('\n').collect();
                        if parts.len() == 2 && !parts[1].trim().is_empty() {
                            let name = parts[0].trim().to_string();
                            let path = parts[1].trim().to_string();
                            
                            if let Err(e) = self.config.add_link(name, path) {
                                eprintln!("Error adding link: {}", e);
                            } else {
                                self.config.save()?;
                            }
                            
                            self.input_buffer.clear();
                            return Ok(true);
                        }
                    }
                }
            }
            KeyCode::Backspace => {
                if self.input_buffer.ends_with('\n') && self.add_mode_field == AddModeField::Path {
                    self.input_buffer.pop();
                    self.add_mode_field = AddModeField::Name;
                } else {
                    self.input_buffer.pop();
                }
            }
            KeyCode::Char(c) => {
                self.input_buffer.push(c);
            }
            _ => {}
        }
        Ok(false)
    }

    fn handle_remove_mode_input(&mut self, key: KeyCode) -> Result<bool> {
        match key {
            KeyCode::Esc => return Ok(true),
            KeyCode::Enter => {
                if let Some(package_name) = self.get_package_at_index(self.selected_index) {
                    self.config.remove_link(&package_name)?;
                    self.config.save()?;
                    if self.selected_index >= self.get_total_items() && self.selected_index > 0 {
                        self.selected_index -= 1;
                    }
                }
                return Ok(true);
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if self.selected_index > 0 {
                    self.selected_index -= 1;
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.selected_index < self.get_total_items().saturating_sub(1) {
                    self.selected_index += 1;
                }
            }
            _ => {}
        }
        Ok(false)
    }

    fn handle_link_mode_input(&mut self, key: KeyCode) -> Result<bool> {
        match key {
            KeyCode::Esc => return Ok(true),
            KeyCode::Enter => {
                if let Some(package_name) = self.get_package_at_index(self.selected_index) {
                    match NpmManager::link_package(&mut self.config, &package_name) {
                        Ok(_) => {
                            self.config.save()?;
                        }
                        Err(e) => {
                            eprintln!("Error linking package: {}", e);
                        }
                    }
                }
                return Ok(true);
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if self.selected_index > 0 {
                    self.selected_index -= 1;
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.selected_index < self.get_total_items().saturating_sub(1) {
                    self.selected_index += 1;
                }
            }
            _ => {}
        }
        Ok(false)
    }

    fn handle_unlink_mode_input(&mut self, key: KeyCode) -> Result<bool> {
        match key {
            KeyCode::Esc => return Ok(true),
            KeyCode::Enter => {
                if let Some(package_name) = self.get_package_at_index(self.selected_index) {
                    match NpmManager::unlink_package(&mut self.config, &package_name) {
                        Ok(_) => {
                            self.config.save()?;
                        }
                        Err(e) => {
                            eprintln!("Error unlinking package: {}", e);
                        }
                    }
                }
                return Ok(true);
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if self.selected_index > 0 {
                    self.selected_index -= 1;
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.selected_index < self.get_total_items().saturating_sub(1) {
                    self.selected_index += 1;
                }
            }
            _ => {}
        }
        Ok(false)
    }

    fn handle_build_mode_input(&mut self, key: KeyCode) -> Result<bool> {
        match key {
            KeyCode::Esc => return Ok(true),
            KeyCode::Enter => {
                if let Some(package_name) = self.get_package_at_index(self.selected_index) {
                    if let Some(status) = self.package_status.get(&package_name) {
                        if status.is_angular_lib {
                            // Extract library name from package name for ng build
                            let lib_name = if let Some(workspace) = &self.angular_workspace {
                                // Try to find matching library name in workspace
                                workspace.projects.iter()
                                    .find(|(_, project)| project.project_type == "library")
                                    .map(|(name, _)| name.clone())
                                    .unwrap_or_else(|| package_name.clone())
                            } else {
                                package_name.clone()
                            };
                            
                            let _ = std::process::Command::new("ng")
                                .args(&["build", &lib_name])
                                .current_dir(&self.workspace_root)
                                .status();
                        }
                    }
                }
                return Ok(true);
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if self.selected_index > 0 {
                    self.selected_index -= 1;
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.selected_index < self.get_total_items().saturating_sub(1) {
                    self.selected_index += 1;
                }
            }
            _ => {}
        }
        Ok(false)
    }

    fn handle_test_mode_input(&mut self, key: KeyCode) -> Result<bool> {
        match key {
            KeyCode::Esc => return Ok(true),
            KeyCode::Enter => {
                if let Some(package_name) = self.get_package_at_index(self.selected_index) {
                    if let Some(status) = self.package_status.get(&package_name) {
                        if status.is_angular_lib {
                            // Extract library name from package name for ng test
                            let lib_name = if let Some(workspace) = &self.angular_workspace {
                                workspace.projects.iter()
                                    .find(|(_, project)| project.project_type == "library")
                                    .map(|(name, _)| name.clone())
                                    .unwrap_or_else(|| package_name.clone())
                            } else {
                                package_name.clone()
                            };
                            
                            let _ = std::process::Command::new("ng")
                                .args(&["test", &lib_name, "--watch=false"])
                                .current_dir(&self.workspace_root)
                                .status();
                        }
                    }
                }
                return Ok(true);
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if self.selected_index > 0 {
                    self.selected_index -= 1;
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.selected_index < self.get_total_items().saturating_sub(1) {
                    self.selected_index += 1;
                }
            }
            _ => {}
        }
        Ok(false)
    }

    fn ui(&mut self, f: &mut Frame) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(0),
                Constraint::Length(3),
            ])
            .split(f.size());

        self.render_header(f, chunks[0]);
        self.render_main_content(f, chunks[1]);
        self.render_footer(f, chunks[2]);

        if self.mode == AppMode::Help {
            self.render_help_popup(f);
        }
    }

    fn render_header(&self, f: &mut Frame, area: Rect) {
        let title = match self.mode {
            AppMode::Normal => {
                let workspace_info = if self.angular_workspace.is_some() {
                    " (Angular Workspace)"
                } else {
                    ""
                };
                format!("Spine - Package Link Manager{}", workspace_info)
            },
            AppMode::AddPackage => "Add Package Link".to_string(),
            AppMode::RemovePackage => "Remove Package Link".to_string(),
            AppMode::LinkPackage => "Link Package to Current Project".to_string(),
            AppMode::UnlinkPackage => "Unlink Package from Current Project".to_string(),
            AppMode::BuildPackage => "Build Angular Library".to_string(),
            AppMode::TestPackage => "Test Angular Library".to_string(),
            AppMode::Help => "Help".to_string(),
        };

        let header = Paragraph::new(title)
            .style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::ALL));

        f.render_widget(header, area);
    }

    fn render_main_content(&mut self, f: &mut Frame, area: Rect) {
        match self.mode {
            AppMode::Normal => self.render_enhanced_package_list(f, area),
            AppMode::AddPackage => self.render_add_package_form(f, area),
            AppMode::RemovePackage => self.render_remove_package_list(f, area),
            AppMode::LinkPackage => self.render_action_package_list(f, area, "Link", Color::Green),
            AppMode::UnlinkPackage => self.render_action_package_list(f, area, "Unlink", Color::Red),
            AppMode::BuildPackage => self.render_action_package_list(f, area, "Build", Color::Blue),
            AppMode::TestPackage => self.render_action_package_list(f, area, "Test", Color::Cyan),
            AppMode::Help => {},
        }
    }

    fn render_enhanced_package_list(&mut self, f: &mut Frame, area: Rect) {
        if self.config.links.is_empty() {
            let empty_msg = Paragraph::new("No package links configured.\nPress 'a' to add a new link, 'h' for help.")
                .alignment(Alignment::Center)
                .wrap(Wrap { trim: true })
                .block(Block::default().borders(Borders::ALL).title("Package Links"));
            f.render_widget(empty_msg, area);
            return;
        }

        let mut items = Vec::new();
        let mut current_index = 0;
        
        // Sort packages alphabetically by name
        let mut sorted_links: Vec<_> = self.config.links.values().collect();
        sorted_links.sort_by(|a, b| a.name.cmp(&b.name));
        
        for link in sorted_links {
            let version = link.version.as_deref().unwrap_or("unknown");
            let status = self.package_status.get(&link.name);
            
            // Health indicator
            let health_icon = if let Some(status) = status {
                match &status.health {
                    HealthStatus::Healthy => "✅",
                    HealthStatus::Warning(_) => "⚠️",
                    HealthStatus::Broken(_) => "❌",
                }
            } else {
                "❓"
            };
            
            // Link status indicator  
            let link_icon = if let Some(status) = status {
                match status.link_status {
                    LinkStatus::Linked => "[🔗 LINKED]",
                    LinkStatus::Unlinked => "[🔓 UNLINKED]",
                    LinkStatus::Unknown => "[❓ UNKNOWN]",
                }
            } else {
                "[❓ UNKNOWN]"
            };
            
            // Angular library indicator
            let lib_icon = if let Some(status) = status {
                if status.is_angular_lib { " 🅰️" } else { "" }
            } else {
                ""
            };
            
            let main_content = format!("{} {} {} (v{}){} -> {}", 
                health_icon, link_icon, link.name, version, lib_icon, link.path.display());
            
            let style = if current_index == self.selected_index {
                Style::default().bg(Color::Blue).fg(Color::White)
            } else {
                Style::default()
            };
            
            items.push(ListItem::new(main_content).style(style));
            current_index += 1;
            
            // Show health details if there are issues
            if let Some(status) = status {
                if let HealthStatus::Warning(msg) | HealthStatus::Broken(msg) = &status.health {
                    let detail_content = format!("    └─ ⚠️ {}", msg);
                    let detail_style = Style::default().fg(Color::Red);
                    items.push(ListItem::new(detail_content).style(detail_style));
                    current_index += 1;
                }
            }
            
            if !link.linked_projects.is_empty() {
                for project_path in &link.linked_projects {
                    let project_content = format!("    └─ 🔗 Linked to: {}", project_path.display());
                    let project_style = Style::default().fg(Color::Gray);
                    items.push(ListItem::new(project_content).style(project_style));
                    current_index += 1;
                }
            }
        }

        // Enhanced title with summary
        let healthy_count = self.package_status.values().filter(|s| matches!(s.health, HealthStatus::Healthy)).count();
        let warning_count = self.package_status.values().filter(|s| matches!(s.health, HealthStatus::Warning(_))).count();
        let broken_count = self.package_status.values().filter(|s| matches!(s.health, HealthStatus::Broken(_))).count();
        let linked_count = self.package_status.values().filter(|s| s.link_status == LinkStatus::Linked).count();
        
        let title = format!("Package Links ({}📦 | {}🔗 | {}✅ | {}⚠️ | {}❌)", 
            self.config.links.len(), linked_count, healthy_count, warning_count, broken_count);

        let list = List::new(items)
            .block(Block::default().borders(Borders::ALL).title(title))
            .highlight_style(Style::default().bg(Color::Blue).fg(Color::White));

        let mut state = ListState::default();
        state.select(Some(self.selected_index));

        f.render_stateful_widget(list, area, &mut state);
    }

    fn render_action_package_list(&mut self, f: &mut Frame, area: Rect, action: &str, color: Color) {
        let mut items = Vec::new();
        let mut current_index = 0;
        
        // Sort packages alphabetically by name
        let mut sorted_links: Vec<_> = self.config.links.values().collect();
        sorted_links.sort_by(|a, b| a.name.cmp(&b.name));
        
        for link in sorted_links {
            let version = link.version.as_deref().unwrap_or("unknown");
            let status = self.package_status.get(&link.name);
            
            // Filter for action-appropriate packages
            let should_show = match action {
                "Build" | "Test" => status.map(|s| s.is_angular_lib).unwrap_or(false),
                _ => true,
            };
            
            if !should_show {
                continue;
            }
            
            // Health indicator
            let health_icon = if let Some(status) = status {
                match &status.health {
                    HealthStatus::Healthy => "✅",
                    HealthStatus::Warning(_) => "⚠️",
                    HealthStatus::Broken(_) => "❌",
                }
            } else {
                "❓"
            };
            
            // Link status for link/unlink actions
            let link_status_text = if action == "Link" || action == "Unlink" {
                if let Some(status) = status {
                    match status.link_status {
                        LinkStatus::Linked => " [CURRENTLY LINKED]",
                        LinkStatus::Unlinked => " [NOT LINKED]",
                        LinkStatus::Unknown => " [STATUS UNKNOWN]",
                    }
                } else {
                    " [STATUS UNKNOWN]"
                }
            } else {
                ""
            };
            
            let content = format!("{} {} (v{}){} -> {}", 
                health_icon, link.name, version, link_status_text, link.path.display());
            
            let style = if current_index == self.selected_index {
                Style::default().bg(color).fg(Color::White)
            } else {
                Style::default()
            };
            
            items.push(ListItem::new(content).style(style));
            current_index += 1;
        }

        let title = format!("Select Package to {} (Enter to confirm, Esc to cancel)", action);
        let list = List::new(items)
            .block(Block::default().borders(Borders::ALL).title(title))
            .highlight_style(Style::default().bg(color).fg(Color::White));

        let mut state = ListState::default();
        state.select(Some(self.selected_index));

        f.render_stateful_widget(list, area, &mut state);
    }

    fn render_add_package_form(&self, f: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Length(3), Constraint::Min(0)])
            .split(area);

        let parts: Vec<&str> = self.input_buffer.split('\n').collect();
        let name_value = parts.get(0).unwrap_or(&"").to_string();
        let path_value = parts.get(1).unwrap_or(&"").to_string();

        let name_style = if self.add_mode_field == AddModeField::Name {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default()
        };

        let path_style = if self.add_mode_field == AddModeField::Path {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default()
        };

        let name_input = Paragraph::new(name_value)
            .block(Block::default().borders(Borders::ALL).title("Package Name").style(name_style));

        let path_input = Paragraph::new(path_value)
            .block(Block::default().borders(Borders::ALL).title("Local Path").style(path_style));

        let instructions = Paragraph::new("Enter package name, then path. Press Enter to confirm each field, Esc to cancel.")
            .wrap(Wrap { trim: true })
            .block(Block::default().borders(Borders::ALL).title("Instructions"));

        f.render_widget(name_input, chunks[0]);
        f.render_widget(path_input, chunks[1]);
        f.render_widget(instructions, chunks[2]);
    }

    fn render_remove_package_list(&mut self, f: &mut Frame, area: Rect) {
        let mut items = Vec::new();
        let mut current_index = 0;
        
        // Sort packages alphabetically by name
        let mut sorted_links: Vec<_> = self.config.links.values().collect();
        sorted_links.sort_by(|a, b| a.name.cmp(&b.name));
        
        for link in sorted_links {
            let content = format!("{} -> {}", link.name, link.path.display());
            let style = if current_index == self.selected_index {
                Style::default().bg(Color::Red).fg(Color::White)
            } else {
                Style::default()
            };
            items.push(ListItem::new(content).style(style));
            current_index += 1;
            
            if !link.linked_projects.is_empty() {
                for project_path in &link.linked_projects {
                    let project_content = format!("  └─ Linked to: {}", project_path.display());
                    let project_style = if current_index == self.selected_index {
                        Style::default().bg(Color::Red).fg(Color::White)
                    } else {
                        Style::default().fg(Color::Gray)
                    };
                    items.push(ListItem::new(project_content).style(project_style));
                    current_index += 1;
                }
            }
        }

        let list = List::new(items)
            .block(Block::default().borders(Borders::ALL).title("Select Package to Remove (Enter to confirm, Esc to cancel)"))
            .highlight_style(Style::default().bg(Color::Red).fg(Color::White));

        let mut state = ListState::default();
        state.select(Some(self.selected_index));

        f.render_stateful_widget(list, area, &mut state);
    }

    fn render_footer(&self, f: &mut Frame, area: Rect) {
        let help_text = match self.mode {
            AppMode::Normal => {
                if self.angular_workspace.is_some() {
                    "q: Quit | h: Help | a: Add | r: Remove | l: Link | u: Unlink | b: Build | t: Test | F5: Refresh"
                } else {
                    "q: Quit | h: Help | a: Add | r: Remove | l: Link | u: Unlink | F5: Refresh"
                }
            },
            AppMode::AddPackage => "Enter: Next/Confirm | Esc: Cancel | Backspace: Delete",
            AppMode::RemovePackage => "Enter: Remove Selected | Esc: Cancel | ↑↓/jk: Navigate",
            AppMode::LinkPackage => "Enter: Link Selected | Esc: Cancel | ↑↓/jk: Navigate",
            AppMode::UnlinkPackage => "Enter: Unlink Selected | Esc: Cancel | ↑↓/jk: Navigate",
            AppMode::BuildPackage => "Enter: Build Selected | Esc: Cancel | ↑↓/jk: Navigate",
            AppMode::TestPackage => "Enter: Test Selected | Esc: Cancel | ↑↓/jk: Navigate",
            AppMode::Help => "Press h, q, or Esc to close help",
        };

        let footer = Paragraph::new(help_text)
            .style(Style::default().fg(Color::Cyan))
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::ALL));

        f.render_widget(footer, area);
    }

    fn render_help_popup(&self, f: &mut Frame) {
        let area = centered_rect(60, 70, f.size());
        f.render_widget(Clear, area);

        let help_text = vec![
            Line::from(vec![Span::styled("Spine Enhanced Interactive Mode", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))]),
            Line::from(""),
            Line::from("Navigation:"),
            Line::from("  ↑/k        - Move up"),
            Line::from("  ↓/j        - Move down"),
            Line::from(""),
            Line::from("Package Management:"),
            Line::from("  a          - Add new package link"),
            Line::from("  r/Delete   - Remove selected package link"),
            Line::from("  l          - Link package to current project"),
            Line::from("  u          - Unlink package from current project"),
            Line::from(""),
            Line::from("Angular Development (if workspace detected):"),
            Line::from("  b          - Build selected Angular library"),
            Line::from("  t          - Test selected Angular library"),
            Line::from(""),
            Line::from("System:"),
            Line::from("  h          - Show this help"),
            Line::from("  F5         - Refresh package status"),
            Line::from("  q/Esc      - Quit application"),
            Line::from(""),
            Line::from("Status Indicators:"),
            Line::from("  ✅ - Package healthy    ⚠️ - Warning    ❌ - Broken"),
            Line::from("  🔗 - Linked            🔓 - Not linked  🅰️ - Angular lib"),
            Line::from(""),
            Line::from("About:"),
            Line::from("Enhanced interactive mode with live status monitoring,"),
            Line::from("health checking, and Angular workspace integration."),
            Line::from(""),
            Line::from("Press h, q, or Esc to close this help."),
        ];

        let help_paragraph = Paragraph::new(help_text)
            .block(Block::default().borders(Borders::ALL).title("Help"))
            .wrap(Wrap { trim: true });

        f.render_widget(help_paragraph, area);
    }
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}