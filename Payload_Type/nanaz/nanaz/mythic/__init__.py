import sys
from importlib import import_module
from pathlib import Path

current_path = Path(__file__).resolve()
parent_dir = current_path.parent
if str(parent_dir) not in sys.path:
    sys.path.append(str(parent_dir))

agent_dir = parent_dir / "agent_functions"

if agent_dir.is_dir():
    load_errors = []
    for file_path in agent_dir.rglob("*.py"):
        if file_path.name == "__init__.py" or file_path.stem.startswith("_"):
            continue

        try:
            relative_path = file_path.relative_to(parent_dir)
            sub_module = ".".join(relative_path.with_suffix("").parts)
            full_module_name = f"{__name__}.{sub_module}"

            import_module(full_module_name)
            print(f"[+] Successfully loaded Mythic component/command: {sub_module}")

        except Exception as e:
            msg = f"[-] Failed to dynamically load module [{file_path.name}]: {e}"
            print(msg)
            load_errors.append(msg)
    if load_errors:
        raise RuntimeError("\n".join(load_errors))
else:
    print(f"[-] Error: Directory not found: {agent_dir}")
    raise RuntimeError(f"agent_functions directory not found: {agent_dir}")
