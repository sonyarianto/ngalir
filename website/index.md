---
layout: home

hero:
  name: Ngalir
  text: Flow automation engine, built in Rust
  tagline: Declarative YAML DAGs. Standalone node binaries. Production-ready observability.
  actions:
    - theme: brand
      text: Get Started
      link: /guide/install
    - theme: alt
      text: View on GitHub
      link: https://github.com/sonyarianto/ngalir

features:
  - title: Declarative DAGs
    details: Describe your workflow as a YAML DAG — no code, no orchestration boilerplate. Nodes run in topological order with bounded concurrency.
  - title: 30+ Node Types
    details: Databases (PostgreSQL, MySQL, SQLite), AI/LLM, HTTP, file formats (CSV, Excel, Parquet, XML, YAML, JSON, ZIP), and integrations (Slack, Telegram, Discord, Notion, Stripe, S3, Airtable, Twilio).
  - title: Credential Vault
    details: Structured credential store with AES-256-GCM encryption, OAuth2 flow, and dynamic UI forms. Reference credentials as vault:// URIs in flows.
  - title: Web UI
    details: Svelte 5 flow editor with drag-and-drop, real-time execution via WebSocket, step-through debugging, and snapshot comparison.
  - title: Production Ready
    details: Prometheus metrics, health endpoints, checkpoint/resume, Docker images, and shell completions. Built for unattended batch jobs.
  - title: AI-Native
    details: Generate flows from natural language prompts with ngalir generate. AI-powered optimization with cost estimation and retry suggestions.
---
