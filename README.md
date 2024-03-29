# Akari: VMM-based macOS Native Container Runtime

Akari is an experimental OCI runtime aims to run macOS native containers on macOS. This runtime works as a standalone VMM using Virtualization.framework. The goal of this project is to support the OCI runtime specification to easily integrate with existing container ecosystem.

It is still in the early stage of development and not ready for production use.

## Requirements

- Apple Silicon (arm64) Mac
- macOS 14.0 or later

## Build

```shell
cargo build && codesign -f --entitlement runtime.entitlements -s - target/debug/akari
# or
make build
```

## License

Akari is licensed under the Apache License, Version 2.0. See [LICENSE](LICENSE) for the full license text.
