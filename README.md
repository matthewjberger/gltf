# glTF Viewer

A physically-based rendering (PBR) viewer for glTF 2.0 models built with the [Nightshade](https://github.com/matthewjberger/nightshade) game engine.

## Features

- **glTF 2.0 Support**: Load and view `.gltf` and `.glb` files
- **PBR Rendering**: Physically-based materials with metallic-roughness workflow
- **HDR Skyboxes**: Load custom `.hdr` environment maps via drag and drop
- **Auto-Centering**: Models are automatically centered and scaled to fit the viewport
- **Post Processing**: Bloom and SSAO effects
- **Color Grading**: Multiple presets and customizable tonemapping
- **Debug Modes**: PBR debug visualization (base color, normals, metallic, roughness, etc.)
- **Procedural Atmospheres**: Built-in procedural skybox options

## Usage

### Drag and Drop

- **glTF/GLB files**: Drop a `.gltf` or `.glb` file onto the window to load a new model (replaces current model)
- **HDR files**: Drop an `.hdr` file to add it as a custom skybox option

### Controls

- **Left Mouse + Drag**: Orbit camera
- **Right Mouse + Drag**: Pan camera
- **Scroll Wheel**: Zoom in/out
- **Arrow Keys**: Cycle through atmospheres
- **Q / Escape**: Exit

### Settings Panel

- **Skybox**: Select from default HDR, custom HDR skyboxes, or procedural atmospheres
- **Color Grading**: Adjust tonemap, gamma, saturation, brightness, contrast
- **Model**: Control rotation speed, reset camera
- **Post Processing**: Toggle bloom and SSAO with adjustable parameters
- **Debug**: PBR debug modes, texture stripe visualization, grid toggle

## Quickstart

```bash
# Native
just run

# WASM (WebGPU)
just run-wasm

# OpenXR (VR headset)
just run-openxr
```

## Prerequisites

* [just](https://github.com/casey/just)
* [trunk](https://trunkrs.dev/) (for web builds)

> Run `just` with no arguments to list all commands

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.
