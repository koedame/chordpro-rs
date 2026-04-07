# Scoop manifest template

`chordsketch.json.template` is the template that `post-release.yml` renders
into the published manifest at
`koedame/scoop-bucket:bucket/chordsketch.json` on every release.

## Version coupling

The `checkver` block uses Scoop's GitHub shorthand:

```json
"checkver": {
  "github": "https://github.com/koedame/chordsketch"
}
```

…which picks up the latest GitHub release tag (e.g. `v0.1.0`). The
`autoupdate.architecture.64bit.url` and `hash.url` then reference
`v$version`, hardcoding the `v` prefix.

This works because **all chordsketch releases use `vX.Y.Z` tags**. If a
release ever ships without the `v` prefix, both autoupdate URLs (and the
template's release URL) must be updated to drop the literal `v` and let
Scoop's `$version` handle the prefix instead.

See #1073.

## Adding new architectures

If we add additional `architecture` entries (`32bit`, `arm64`), each one
needs its own `autoupdate.architecture.<arch>` block with a matching
`hash.regex` referencing the appropriate target triple.
