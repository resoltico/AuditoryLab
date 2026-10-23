# Repository-specific guidance

`target/` holds both Cargo compiler caches and retained release/audit evidence. Use the profile/isolated-target cleanup described in [Contributing: build storage and cleanup](CONTRIBUTING.md#build-storage-and-cleanup); inspect retained evidence before deleting the whole tree. Temporary isolated build directories must be removed after required outputs are retained.
