[![Status](https://img.shields.io/badge/status-in--development-red)](https://github.com)
[![Rust](https://img.shields.io/badge/Rust-lang-orange?logo=rust)](https://www.rust-lang.org)
[![License](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](LICENSE)

# graphica
**graphica** is a high-performance, type-safe Frame Graph (Render Graph) architecture implemented in Rust using **Vulkan**. 

It decouples frame rendering logic from explicit resource management by automatically handling synchronization barriers, and layout transitions based on an directed acyclic graph (DAG).

### Core Engine
- **ID/Handle System** - Type-safe abstraction layer preventing errors linked to managing heavy types.
- **Automated Synchronization** - Evaluates the graph topology to automatically insert memory barriers and image layout transitions.
- **Culling** - Prevents operations that won't show up on the screen from execution.
- **Lazy Allocation** - Defers creation of descriptor groups, pipelines, render passes, and framebuffers until the first relevant draw call to minimize VRAM usage.
- **Descriptor Group Caching** - Deduplicates and caches `DescriptorSetGroup` allocations based on internal data.
- **GraphViz Integration** - Exports the runtime execution pipeline into a `.dot` graph format for visual debugging.

## Frame Graph Features
- [ ] Transient resources (aliasing)
- [ ] Flat appending (sub-graph appending)
- [ ] Queue parallelization and synchronization

## Run requirements
* **Mininal MSRV:** 1.88.
* **Vulkan SDK:** Required to run with validation error checks, for that you need to set environment variable *ValidationLayer* to "true"/1. 

## Examples
To run examples enter this command:
``` sh
cargo run --example <example_name> 
```
> [!Caution]
>
> Some examples require special features. If an example fails to initialize, check the source file header for required feature flags and run accordingly. 
> ``` sh
> cargo run --example <example_name> --features "<feature_name>"
> ```

## **Licensing**
This project is [licensed](LICENSE) under **Apache-2.0**.

Copyright 2026 **Mironc**  
Contact: Mironc.dev@gmail.com

## **Contributing**

If you want to contribute, feel free to fork the repository and open a Pull Request.
