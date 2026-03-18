---
title: CLI Commands
description: Complete CLI reference for mxr
---

## Overview

mxr is a single binary with subcommands. Running `mxr` without arguments launches the TUI.

## Commands

| Command | Description |
|---------|-------------|
| `mxr` | Launch TUI (starts daemon if needed) |
| `mxr daemon` | Start daemon explicitly |
| `mxr search <query>` | Search messages |
| `mxr count <query>` | Count matching messages |
| `mxr cat <id>` | Print message body |
| `mxr thread <id>` | Print full thread |
| `mxr headers <id>` | Print message headers |
| `mxr labels` | List labels with counts |
| `mxr saved` | Manage saved searches |
| `mxr sync` | Trigger sync |
| `mxr status` | Daemon health overview |
| `mxr config` | Show resolved config |
| `mxr doctor` | Run diagnostics |
| `mxr version` | Print version |
| `mxr completions <shell>` | Generate shell completions |
