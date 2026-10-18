# 本地交付约定

- 用户要求只在本地实现和验证，未经后续明确授权不要发布软件。
- 本地交付程序统一为 `交付/人大校园网助手.exe`。不要为每次修改新增版本目录或新的程序文件名。
- 编译完成后使用 `powershell -ExecutionPolicy Bypass -File scripts/deploy-local.ps1`：退出本项目的旧助手、替换固定位置的程序、校验并打开新版。用户已明确授权这一退出与重启流程，无需每次再次确认。
- 用户数据统一存放在 `%LOCALAPPDATA%/cn.ruc.webhelper`，交付仅保留一个 EXE。旧目录数据由程序自动复制迁移；核实后将旧副本归档到 `.local/retired-delivery-data/`，不要删除用户数据。
- 清理临时交付目录时，先保留其中的用户数据，再移除重复程序和空目录。
