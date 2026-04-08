# Docs directory description
`docs/` is the official governance document directory for CodexManager and is no longer considered a temporary repository.

Goal:
- Let structural governance, decision-making records, release instructions, and operation manuals all enter the main line of the warehouse
- Allow new collaborators to find the correct document without relying on verbal handoffs

## Document responsibility boundaries
- Root directory `README.md` / `README.en.md`: Home navigation, project overview, quick start.
- Root directory `CHANGELOG.md`: The only source of truth for version history and unreleased updates.
- `report/*`: Operation, troubleshooting, compatibility, FAQ manuals.
- `release/*`: Build, release, product and release process description.
- `docs/plan/*` / `docs/decision/*`: Long-term governance, implementation plans and record of decisions.

##Latest update entry
- If you want to see the latest released content and unreleased updates to the main branch, first read [CHANGELOG.md](CHANGELOG.md).
- If you want to quickly determine "which document is most suitable for the current problem", first look at the "Document Home Page" and "Recommended Entry" below.

## Document Home Page
| If you want to do something now | It is recommended to read it first |
| --- | --- |
| Run the project, deploy service/web, process macOS First startup | [Run and Deployment Guide](report/runtime-and-deployment-guide.md) |
| Configure environment variables, database, port, proxy, listening address | [Environment variables and running configuration instructions](report/environment-and-runtime-config.md) |
| Troubleshoot account misses, import exceptions, and challenge interceptions | [FAQ and account hit rules](report/faq-and-account-routing-rules.md) |
| Troubleshoot background task account skipping, disabling and deactivation reasons | [Background task account skipping instructions](report/background-task-account-skip-notes.md) |
| Plug-in Center Minimum Access and Quick Connection | [Plug-in Center Minimum Access Instructions](report/plugin-center-minimal-integration.md) |
| System internal interface summary list, all accessible interfaces | [System internal interface summary list ](report/system-internal-interface-inventory.md) |
| Local packaging, workflow release, download product | [Build release and script instructions](release/build-release-and-scripts.md) |

## Directory division of labor

### `docs/plan/`
Used to save implementation plans, governance lists, and staged TODOs.

### `docs/decision/`
Used to maintain decision records and ADRs.

### `release/`
Used to save release notes, rollback plans, release acceptance records, and build release manuals.

### `report/`
Used to save scan, troubleshooting, operational, compatibility, FAQ type reports and manuals.

## Recommended entrance

### Run and use
| Documentation | Function |
| --- | --- |
| [Operation and Deployment Guide.md](report/runtime-and-deployment-guide.md) | First boot, Service version, Docker, macOS First boot |
| [Environment variables and running configuration instructions.md](report/environment-and-runtime-config.md) | View all running configurations, default values and functions in one place |
| [FAQ and Account Hitting Rules.md](report/faq-and-account-routing-rules.md) | Frequently Asked Questions, Account Hitting and Log Troubleshooting |
| [Comparison table between the current gateway and Codex official request parameters.md](report/gateway-vs-codex-official-params.md) | Comparison table of the actual outbound parameters of the current gateway, target Codex parameters, and the differences between the two |
| [Instructions for skipping background task accounts.md](report/background-task-account-skip-notes.md) | Background task filtering, account disabling, workspace Reasons for deactivation |
| [Minimum Troubleshooting Manual.md](report/minimal-troubleshooting-guide.md) | Quickly locate the most common startup and forwarding problems |
| [Plug-in Center Minimum Access Instructions.md](report/plugin-center-minimal-integration.md) | Plug-in Center Minimum Access Fields, Interfaces and Rhai Minimum Functions |
| [Difference table of request headers and parameters between the current gateway and Codex.md](report/gateway-vs-codex-headers-and-params.md) | Comparison of parameter transfer, request headers and parameters between the current gateway and Codex |
| [Plug-in Center docking and interface list.md](report/plugin-center-integration-and-interfaces.md) | Plug-in Center access method, market model, RPC/Tauri commands, list fields, Rhai interface list |
| [General list of system internal interfaces.md](report/system-internal-interface-inventory.md) | All system interfaces can be connected to internal interfaces, Tauri/RPC comparison, and plug-in built-in functions |

### Release and Build
| Documentation | Function |
| --- | --- |
| [Build release and script instructions.md](release/build-release-and-scripts.md) | Local build, script parameters, workflow entry |
| [Release and product description.md](release/release-and-artifacts.md) | Description of products, naming and release results of each platform |
| [Contrast between scripting and publishing responsibilities.md](report/script-and-release-responsibility-matrix.md) | Scripting responsibility boundaries and usage scenarios |

### Governance and decision-making
| Documentation | Function |
| --- | --- |

## Submit rules

### Documentation that should be committed to Git
- Still valuable for future collaborators
- Will affect subsequent development, testing, release or troubleshooting methods
- Can be used as part of the long-term source of truth for the project

### Documentation not recommended for submission to Git
- Temporary draft
- Personal process notes
- Disposable intermediate products
- Local test records

## Ignore rules
The current repository ignores the following documents:
- `docs/**/*.tmp.md`
- `docs/**/*.local.md`

If it is a formal document, do not use the above suffix.

## Naming suggestions
Recommended format:

```text
Long-term retention document: theme.md
One-time report: yyyyMMddHHmmssfff_topic.md
```

## Maintenance Agreement
- When adding important governance documents, give priority to `docs/` and do not continue to pile them into README.
- Version history continues in `CHANGELOG.md`.
- The architecture overview continues to be maintained in `ARCHITECTURE.md`.
- Collaboration specifications continue to be maintained at `CONTRIBUTING.md`.
- Do not write unreleased updates into multiple long documents at the same time; when external explanation is needed, add `CHANGELOG.md` first, and README only retains the summary and entry.

## Contact information
- Telegram Group chat: [CodexManager TG group](https://t.me/+OdpFa9GvjxhjMDhl)