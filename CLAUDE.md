# Lantir — Claude Code Guide

Lantir is a Vulkan/HLSL PBR rendering engine integrated with Bevy. This file
describes the project layout and the autonomous debugging workflow Claude uses
to investigate rendering bugs without human intervention.

> **MANDATORY RULE FOR CLAUDE**: Any request to implement a new rendering
> feature (new pass, new extension, new visual effect, significant new code)
> **MUST** be handled by invoking the `feature-pipeline` agent. Do not write
> rendering feature code directly. The pipeline is:
> `render-architect → implementer → rendering-reviewer → invariant-auditor →
> visual-debug-tester → simplifier → gatekeeper`

---

## Quick-start commands

| Goal | Command |
|------|---------|
| Build (debug) | `scripts/build.sh` |
| Build (release) | `scripts/build.sh release` |
| Run interactive example | `scripts/run.sh` |
| Dump one debug frame | `scripts/dump-frame.sh` |
| Collect logs (10 s) | `scripts/collect-logs.sh 10` |
| Compare to baseline | `scripts/compare-frames.sh` |
| Set new baseline | `scripts/set-baseline.sh` |
| Kill running process | `scripts/stop.sh` |

All scripts are executable and self-contained. Run them from the repo root or
let the script `cd` there automatically.

---

## Crate layout

```
crates/
  lantir_hal/        Vulkan abstraction (device, swapchain, command buffers,
                     barriers, buffers, images, pipelines)
  lantir_render/     World renderer, render passes, resource manager,
                     Bevy plugin, HLSL shaders
  lantir_gltf/       glTF / GLB scene loader (uploads meshes + textures)
  lantir_bevy/       LantirDefaultPlugins (window + winit + render + log)
examples/
  src/main.rs        Interactive driving demo (Porsche + drift track)
  src/debug_scene.rs Fixed-camera debug binary — renders one frame + exits
  src/camera.rs      Third-person chase camera plugin
  assets/            GLB models, EXR skybox
```

---

## Render pipeline

```
Bevy Update → render_system()
               ↓
         WorldRenderer::draw_frame(&scene)
               ↓
         [barrier] UNDEFINED → COLOR_ATTACHMENT_OPTIMAL  (color_target)
         [barrier] UNDEFINED → DEPTH_ATTACHMENT_OPTIMAL  (depth_target)
               ↓
         SkyPass::execute()       ← clears color + depth, draws skybox
         PbrPass::execute(Opaque)
         PbrPass::execute(Masked)  ← alpha-test via specialization constant 0
         PbrPass::execute(Transparent) ← sorted back-to-front
               ↓
         [barrier] COLOR_ATTACHMENT → TRANSFER_SRC  (color_target)
         [barrier] UNDEFINED → TRANSFER_DST  (swapchain image)
         cmd_copy_image (blit to swapchain)
         [barrier] TRANSFER_DST → PRESENT_SRC  (swapchain image)
               ↓
         submit_and_present()
```

**Key invariants:**
- Reverse depth: `GREATER_OR_EQUAL`, clear to `0.0`, proj z at infinity = 0
- Color format: `B8G8R8A8_UNORM` (bytes in memory: B G R A)
- HDR tonemapping (Reinhard) happens inside `sky.hlsl`
- Normal matrix: `inverse().transpose()` of model matrix

---

## Frame dump (deterministic debug mode)

```
LANTIR_DUMP_FRAME=debug/frames/out.png cargo run --bin debug_scene
```

The `debug_scene` binary:
1. Spawns a fixed camera at `eye=(5,3,8)` looking at `(0,1,0)`
2. Loads `basicmesh.glb` + `sunset.exr` skybox
3. Waits 3 frames for GPU init
4. Calls `WorldRenderer::dump_frame_to_file()` which:
   - Calls `device_wait_idle()`
   - `immediate_submit`: `vkCmdCopyImageToBuffer` from `color_target`
   - Maps buffer, swaps BGRA→RGBA, saves PNG via `image` crate
5. Exits via `std::process::exit(0)`

Output is written to `debug/frames/`. The last frame is always copied to
`debug/frames/latest.png`.

---

## Debug directories

```
debug/
  frames/    PNG frames from dump-frame.sh (gitignored except .gitkeep)
  logs/      Log captures from collect-logs.sh (gitignored except .gitkeep)
  baseline/  baseline.png used by compare-frames.sh
```

---

## MCP server

```bash
cd mcp
pip install -r requirements.txt
python3 lantir_debug_server.py
```

Add to Claude Code MCP settings:
```json
{
  "mcpServers": {
    "lantir-debug": {
      "command": "python3",
      "args": ["/home/fexolm/git/lantir/mcp/lantir_debug_server.py"]
    }
  }
}
```

Tools exposed: `build`, `run`, `dump_frame`, `read_frame`, `collect_logs`,
`compare_frames`, `set_baseline`, `stop`, `read_file`, `grep_source`.

---

## Feature request pipeline

**Any request to add a new rendering feature MUST go through the 7-agent pipeline.**

Trigger phrases: "добавь", "реализуй", "implement", "add a pass", "add support for", "new render pass", "new feature", or any request that implies writing significant new rendering code.

### How to invoke
Use the `feature-pipeline` agent via the Agent tool:
```
Agent(subagent_type="feature-pipeline", prompt="Feature: <description>. <any extra context>")
```

The pipeline runs in this order, automatically:
```
render-architect → implementer → rendering-reviewer → invariant-auditor
       ↑ fix loop ↓                    ↑ fix loop ↓
visual-debug-tester → simplifier → gatekeeper
```

Never skip stages. Never write feature code directly without going through the pipeline.

### Agent roster

| Agent | Role | Invoke when |
|-------|------|-------------|
| `feature-pipeline` | **Coordinator** — runs all stages | User requests a new feature |
| `render-architect` | Plan only, no code | First stage of pipeline |
| `implementer` | Write all Rust + HLSL code | Second stage, and fix loops |
| `rendering-reviewer` | Review code correctness | Third stage |
| `invariant-auditor` | Check Vulkan spec violations | Fourth stage |
| `visual-debug-tester` | Build + run + inspect frame | Fifth stage |
| `simplifier` | Remove unnecessary complexity | Sixth stage |
| `gatekeeper` | Go/no-go + commit message | Final stage |

### Debug agents (separate from feature pipeline)

| Agent | When to invoke |
|-------|---------------|
| `render-bug-diagnosis` | Frame looks wrong, crash, artifact |
| `log-analysis` | Parse runtime logs for errors / VUIDs |
| `visual-regression` | Compare before/after frames for a code change |

---

## Autonomous debug workflow (no human needed)

```
1. scripts/build.sh               ← compile; fix errors if any
2. scripts/dump-frame.sh          ← render debug/frames/latest.png
3. [visual-regression agent]      ← inspect the frame; compare to baseline
4. scripts/collect-logs.sh 5      ← if something looks wrong, grab logs
5. [log-analysis agent]           ← parse debug/logs/latest.log
6. [render-bug-diagnosis agent]   ← identify root cause in source
7. Edit the relevant source file  ← hook runs cargo check automatically
8. scripts/build.sh               ← full rebuild
9. scripts/dump-frame.sh          ← render new frame
10. scripts/compare-frames.sh     ← verify the fix improved the output
11. scripts/set-baseline.sh       ← promote to baseline when satisfied
```

---

## Build notes

- Shaders are compiled from HLSL at build time by `build.rs` using `dxc`
- `dxc` must be on `$PATH` or inside `$VULKAN_SDK/Bin/dxc`
- The Vulkan SDK installed at `~/.vulkan/` adds `dxc` automatically
- Shader output is cached in `target/`; touch a `.hlsl` file to force recompile
- Edition: Rust 2024 (`resolver = "3"`)

---

## Common pitfalls

| Symptom | Likely cause |
|---------|-------------|
| Black frame | Skybox image not loaded, or color_target not in TRANSFER_SRC_OPTIMAL |
| Depth artifacts | Reverse depth matrix wrong (check `perspective_infinite_reverse_rh` + Y-flip) |
| Wrong BGRA colors | PNG saved without BGRA→RGBA swap |
| Validation crash | Missing image barrier or wrong layout |
| `run_once` runs every frame | `SceneLoaded` resource removal logic broken |
| Physics car teleports | `setup_car_physics_system` ran before scene loaded |
