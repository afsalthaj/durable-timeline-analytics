# Timeline docs

This is the product and design documentation for **durable timeline analytics**.
It is not the Golem platform guide. That lives in the generated section of
[AGENTS.md](../AGENTS.md) (the `golem-managed` block) and in
[learn.golem.cloud](https://learn.golem.cloud).

| Doc | What it is |
|---|---|
| [Landing page](../README.md) | Short pitch, language, how to run the demo |
| [Design notes](design.md) | CIRR walkthrough, architecture, production, scale, compute reuse |
| [Open issues](../ISSUES.md) | What is still unbuilt |

### In `design.md`

1. **CIRR, visually** — how `has_existed` / `has_existed_within` / `latest` / `duration_where` combine
2. **Architecture** — driver, leaves, derived nodes, aggregator; push vs query
3. **Production** — Kafka/Pulsar feeder, one component / many metrics, agent naming
4. **Scale** — agents per session, what is in memory, Kubernetes sketch
5. **Compute reuse** — why `ParentRef` is singular today and how sharing would work
