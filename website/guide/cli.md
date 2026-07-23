# CLI Reference

```
ngalir <COMMAND>

Commands:
  run        Execute a Flow Spec               ngalir run flow.yaml
  nodes      List all na-* on PATH             ngalir nodes
  validate   Validate without running          ngalir validate flow.yaml
  generate   Generate a flow from a prompt     ngalir generate "fetch API → email result"
  skills     List node skills registry (JSON)  ngalir skills | jq .
  search     Search node registry              ngalir search slack
  install    Install a node from registry      ngalir install slack
  optimize   Analyze a flow and suggest improvements
  serve      Start the web UI server
  init-node  Scaffold a new node crate         ngalir init-node
  completion Generate shell completions        ngalir completion bash
  help       Print help

Run flags:
  --input JSON       Seed __request__ with initial data
  --state-dir PATH   Enable checkpoint / resume
  --metrics-port N   Expose /metrics on :N
```
