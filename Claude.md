The aim of this project is to be a command line tool which replaces the "npm link" command, and will provide a more functional way of testing locally created NPM packages. It will revolve around a config.toml file, which will contain configuration such as which local NPM packages are currently linking to which folders.
It needs to be aware of different versions of package.json and work accordingly with them.

The UI and UX of this command line tool is going to be the largest unique selling point. This command line tool should present an interface which can then use the directional arrow keys to view and select the different configuration, and modify it.

This project is written in Rust, and needs to be fast, efficient, and memory safe. There should also be parameter validation and error handling. 
