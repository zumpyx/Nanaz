import glob
import os.path
from pathlib import Path
from importlib import import_module, invalidate_caches
import sys

# 1. 优先修正 sys.path，确保子模块在导入时能正常引用同级或上级目录
currentPath = Path(__file__).resolve()
sys.path.append(os.path.abspath(str(currentPath.parent)))

# 2. 动态计算搜索路径
searchPath = currentPath.parent / "agent_functions" / "*.py"

# 使用 glob 查找所有 Python 脚本
modules = glob.glob(str(searchPath))
invalidate_caches()

for x in modules:
    # 规避初始化文件和非 Python 文件
    if not x.endswith("__init__.py") and x.endswith(".py"):
        try:
            stem_name = Path(x).stem
            # 💡 关键点：只需 import_module 即可！
            # 只要代码被执行，mythic_container 就会自动通过子类钩子注册 PayloadType 或 Command。
            import_module(f"{__name__}.agent_functions.{stem_name}")
            print(f"[+] 成功加载 Mythic 组件/命令: {stem_name}")

        except Exception as e:
            # 防御性升级：如果某个命令写错了，跳过它，防止整个容器直接离线崩溃
            print(f"[-] 动态加载模块失败 {x}: {str(e)}")
            import traceback
            traceback.print_exc()
