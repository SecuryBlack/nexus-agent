---
name: Bug report
about: Create a report to help us improve Nexus Agent
title: '[BUG] '
labels: 'bug'
assignees: ''

---

**Describe the bug**
A clear and concise description of what the bug is.

**To Reproduce**
Steps to reproduce the behavior:
1. Agent configuration used (`agent.toml` with the token redacted)
2. Command run / Service state (`systemctl status securyblack-agent` or `Get-Service NexusAgent`)
3. See error in logs (`journalctl -u securyblack-agent`, or the log file next to `agent.toml`)

**Expected behavior**
A clear and concise description of what you expected to happen.

**Environment (please complete the following information):**
 - Nexus Agent Version: [e.g. 0.2.17] (`nexus-agent --version`)
 - OS & Architecture: [e.g. Ubuntu 22.04 LTS x86_64, Windows Server 2022 AMD64]
 - Enabled local agents: [e.g. oxipulse, ferrosentry, none]

**Logs / Screenshots**
If applicable, attach relevant log snippets from `nexus-agent`.

**Additional context**
Add any other context about the problem here.

