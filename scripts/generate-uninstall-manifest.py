#!/usr/bin/env python3
"""Generate the exact-file NSIS uninstall manifest (for cross builds).

Usage: python scripts/generate-uninstall-manifest.py PACKAGE OUTPUT.nsh
The Windows PowerShell build contains the same generation logic, so Python
is not a prerequisite for building the Windows installer.
"""
from pathlib import Path
import sys


def escape_path(path: Path) -> str:
    text = str(path).replace('/', '\\')
    if any(char in text for char in '\r\n\t'):
        raise ValueError(f'Unsupported control character in package path: {path!r}')
    return text.replace('$', '$$').replace('"', '$\\"')


def generate(package: Path, output: Path) -> None:
    package = package.resolve(strict=True)
    output = output.resolve()
    if output.is_relative_to(package):
        raise ValueError('The generated manifest must be outside the package tree.')
    files = sorted(p.relative_to(package) for p in package.rglob('*') if p.is_file())
    folders = sorted(
        (p.relative_to(package) for p in package.rglob('*') if p.is_dir()),
        key=lambda p: (len(p.parts), str(p)), reverse=True,
    )
    lines = ['; Generated from the exact package contents. Do not hand edit.',
             '!macro RemoveInstalledFiles']
    lines.extend('  Delete "$INSTDIR\\' + escape_path(p) + '"' for p in files)
    lines.extend('  RMDir "$INSTDIR\\' + escape_path(p) + '"' for p in folders)
    lines.append('!macroend')
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text('\n'.join(lines) + '\n', encoding='utf-8')
    print(f'Manifest: {len(files)} owned files; {len(folders)} empty-directory removals.')


if __name__ == '__main__':
    if len(sys.argv) != 3:
        raise SystemExit(__doc__)
    generate(Path(sys.argv[1]), Path(sys.argv[2]))
