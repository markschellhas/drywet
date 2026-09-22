# Agent instructions

<!-- feature-map:start -->
## Feature Map

**ALWAYS** use Feature Map before feature work, debugging, PRDs, or plans.
Do not implement from a cold grep when a map exists.

```bash
./bin/feature-map list
./bin/feature-map search <keyword>
./bin/feature-map find <path-fragment>
./bin/feature-map <feature-name>
```

Maps in `.features/*.yaml` are the authoritative cross-app architecture source.
Keep them dense: fields, not essays.

If `list` is empty or search/find miss the area you are about to change,
**scour the existing code and author maps first**. Cluster by user-visible
capability, not by file. Prefer `entry_points` that exist on disk, then
run `feature-map validate` and `feature-map check`.

Skill: `.agents/skills/feature-map/SKILL.md`, `.claude/skills/feature-map/SKILL.md`
Read it before authoring: `references/existing-repos.md` (playbook),
`references/authoring.md` (fields), `references/example-map.md` (worked example).
<!-- feature-map:end -->
