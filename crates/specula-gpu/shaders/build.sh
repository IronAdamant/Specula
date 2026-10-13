#!/bin/sh
# Compile every shader in this directory to SPIR-V and validate it. Run by hand (no build scripts):
#   sh crates/specula-gpu/shaders/build.sh
# Needs the Vulkan SDK (glslc, spirv-val); VULKAN_SDK defaults to C:/VulkanSDK/1.4.363.0.
set -e
SDK="${VULKAN_SDK:-C:/VulkanSDK/1.4.363.0}"
cd "$(dirname "$0")"
for src in *.vert *.frag; do
    "$SDK/Bin/glslc" -O --target-env=vulkan1.3 "$src" -o "$src.spv"
    "$SDK/Bin/spirv-val" --target-env vulkan1.3 "$src.spv"
done
echo "shaders: ok"
