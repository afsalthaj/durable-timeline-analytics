## Feeder

The feeder reads events from a Pulsar topic and sends each one to the leaf agents
returned by `TimelineDriver::initialize_timeline`. Messages are JSON-encoded
`common_lib::Event` values, as published by `sample-event-producer`.

```shell
LEAF_AGENTS=sess-1-latest-event-to-state-2 GOLEM_APP_DIR=../.. cargo run
```

Both Pulsar samples are excluded from the Cargo workspace (the `pulsar` crate needs `protoc`
to build); run them from their own directories.

## Note
This is only to showcase the various possibilities of the architecture and in real life, the existence of feeder might be in
a different shape, and in-fact user can choose to bring them as part of the Golem Workflow itself.
That said, in most of the situations, a feeder (as well as the event producer) lies outside the Golem workflow
or even outside the network (Example: Another VPC in the same AWS account, or in a different account)
