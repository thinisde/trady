#!/usr/bin/env python3
"""Check canonical JSON copies, skill catalog metadata and local Markdown links."""
import json
from pathlib import Path
import re
from urllib.parse import unquote

import yaml

ROOT = Path(__file__).resolve().parent.parent


def verify_schemas():
    count = 0
    for name, version in [("llm-contracts", 2), ("research-contracts", 1)]:
        blocks = re.findall(r"```json\n(.*?)\n```", (ROOT / "docs/v1" / f"{name}.md").read_text(), re.S)
        expected = json.loads(blocks[0])
        actual = json.loads((ROOT / "schemas" / f"{name}.v{version}.json").read_text())
        assert actual == expected, f"schema differs from normative documentation: {name}"
        count += 1
        for index, block in enumerate(blocks[1:], 1):
            assert json.loads((ROOT / "schemas/fixtures" / f"{name}-{index}.json").read_text()) == json.loads(block)
            count += 1
    for index, block in enumerate(re.findall(r"```json\n(.*?)\n```", (ROOT / "docs/v1/llm-and-tools.md").read_text(), re.S), 1):
        assert json.loads((ROOT / "schemas/fixtures" / f"conversation-{index}.json").read_text()) == json.loads(block)
        count += 1
    return count


def verify_catalog():
    root = ROOT / "skills"
    catalog = yaml.safe_load((root / "catalog.yaml").read_text())
    assert catalog["catalog_version"] == 1
    names = set()
    paths = set()
    sectors = {}
    subsectors = []
    for skill in catalog["skills"]:
        path = Path(skill["path"])
        assert not path.is_absolute() and ".." not in path.parts
        assert skill["name"] not in names and str(path) not in paths
        names.add(skill["name"])
        paths.add(str(path))
        current = root
        for part in path.parts:
            current = current / part
            assert not current.is_symlink()
        assert current.resolve().is_relative_to(root.resolve()) and current.is_file()
        frontmatter = re.match(r"\A---\n(.*?)\n---", current.read_text(), re.S)
        assert frontmatter, f"missing frontmatter: {path}"
        metadata = yaml.safe_load(frontmatter.group(1))
        assert metadata["name"] == skill["name"] and str(metadata["metadata"]["version"]) == str(skill["version"]), path
        if skill["kind"] == "sector":
            code = str(skill["gics_code"])
            assert code not in sectors
            sectors[code] = skill["name"]
        elif skill["kind"] == "subsector":
            subsectors.append(skill)
    assert len(names) == 52 and len(sectors) == 11 and len(subsectors) == 25
    group_codes = set()
    for skill in subsectors:
        group = str(skill["gics_code"])
        parent = str(skill["parent_code"])
        assert parent in sectors and group.startswith(parent) and group not in group_codes
        group_codes.add(group)
    return len(names)


def verify_markdown():
    files = [ROOT / "README.md", ROOT / "index.md"]
    for directory in ["docs", "skills", "crates", "schemas", "migrations", "scripts", "config"]:
        files.extend((ROOT / directory).rglob("*.md"))
    links = 0
    for path in files:
        text = path.read_text()
        fences = re.findall(r"^\s*```", text, re.M)
        assert len(fences) % 2 == 0, f"unclosed fence: {path}"
        # Remove fenced blocks so illustrative paths do not become link assertions.
        prose = re.sub(r"```.*?```", "", text, flags=re.S)
        for destination in re.findall(r"(?<!!)\[[^\]]+\]\(([^)]+)\)", prose):
            destination = destination.strip().split(' "', 1)[0].strip("<>")
            if re.match(r"[a-zA-Z][a-zA-Z0-9+.-]*:", destination):
                continue
            target, _, fragment = destination.partition("#")
            target_path = path.parent / unquote(target) if target else path
            assert target_path.exists(), f"broken link: {path.relative_to(ROOT)} -> {target}"
            if fragment and target_path.suffix == ".md":
                assert unquote(fragment) in heading_anchors(target_path.read_text()), f"broken anchor: {path.relative_to(ROOT)} -> {destination}"
            links += 1
    return len(files), links


def heading_anchors(text):
    anchors = set()
    duplicates = {}
    text = re.sub(r"```.*?```", "", text, flags=re.S)
    for title in re.findall(r"^#{1,6}\s+(.+?)\s*#*\s*$", text, re.M):
        title = re.sub(r"\[([^\]]+)\]\([^)]+\)", r"\1", title).replace("`", "")
        slug = "".join(c for c in title.lower() if c.isalnum() or c in "_- ").replace(" ", "-")
        index = duplicates.get(slug, 0)
        duplicates[slug] = index + 1
        anchors.add(f"{slug}-{index}" if index else slug)
    return anchors


if __name__ == "__main__":
    schema_count = verify_schemas()
    skill_count = verify_catalog()
    docs, links = verify_markdown()
    print(f"Verified {schema_count} canonical JSON assets, {skill_count} skill records, {docs} Markdown files and {links} local links.")
