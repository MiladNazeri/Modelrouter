# Security Policy

## Supported Versions

Modelrouter is currently alpha software. Security fixes target the current `main` branch until versioned releases exist.

## Reporting A Vulnerability

Please open a private security advisory on GitHub if available, or contact the maintainers through the repository issue tracker with a request for a private disclosure channel. Do not publish exploit details before maintainers have had a reasonable chance to respond.

Include:

- Affected version or commit.
- Steps to reproduce.
- Expected impact.
- Whether secrets, provider accounts, local files, or billing data may be exposed.

## Secrets And Billing Data

Modelrouter should be run as a localhost tool unless you have added your own authentication and network hardening. Do not expose the daemon directly to the public internet.

The project reads provider keys from environment variables for spend sync commands. Do not commit `.env` files, request logs, billing exports, or local `modelrouter.toml` files containing private endpoints or account details.

Budget controls are guardrails, not financial guarantees. Provider billing APIs and exports can lag behind real usage.
