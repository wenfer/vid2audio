#!/usr/bin/env python3
"""生成 Tauri v2 updater 更新清单 latest.json（支持多平台合并）。

Tauri 的静态清单会校验整个文件：只要某个平台 key 存在就必须完整（url + signature
都要有），而缺失某个平台的 key 会让那个平台的自动更新直接失败。所以 Windows 和
macOS 的签名必须合并进同一份 latest.json，由一个 job 统一生成，不能用各自平台自己
生成的单平台清单覆盖。

用法：
    python3 make_updater_manifest.py <version> <platform>=<sig-file>:<asset-name> [...]

示例：
    python3 make_updater_manifest.py 0.2.9 \
      "windows-x86_64=build/windows.sig:vid2audio-0.2.9-windows-x64-setup.exe" \
      "darwin-x86_64=build/macos-x86.sig:vid2audio-0.2.9-macos-x86_64.app.tar.gz" \
      "darwin-aarch64=build/macos-arm.sig:vid2audio-0.2.9-macos-aarch64.app.tar.gz" \
      > latest.json

更新清单发布为 GitHub Release 资产 latest.json，应用端 tauri.conf.json 的
updater.endpoints 指向
https://github.com/wenfer/vid2audio/releases/latest/download/latest.json，
所以每次发布都用同名文件覆盖即可。
"""
import datetime
import json
import sys

# 与 tauri.conf.json 的 updater.endpoints 里的仓库一致。
RELEASE_BASE = "https://github.com/wenfer/vid2audio/releases/download"


def parse_spec(spec: str) -> tuple[str, str, str]:
    """解析 "<platform>=<sig-file>:<asset-name>"。"""
    platform, rest = spec.split("=", 1)
    sig_file, asset = rest.split(":", 1)
    return platform, sig_file, asset


def main() -> int:
    if len(sys.argv) < 4:
        print(
            "用法: make_updater_manifest.py <version> <platform>=<sig-file>:<asset-name> [...]",
            file=sys.stderr,
        )
        return 1
    version = sys.argv[1]
    platforms = {}
    for spec in sys.argv[2:]:
        platform, sig_file, asset = parse_spec(spec)
        signature = open(sig_file, encoding="utf-8").read().strip()
        if not signature:
            print(f"签名文件 {sig_file} 为空", file=sys.stderr)
            return 1
        platforms[platform] = {
            "signature": signature,
            "url": f"{RELEASE_BASE}/v{version}/{asset}",
        }

    manifest = {
        "version": version,
        "notes": f"Vid2Audio v{version}",
        "pub_date": datetime.datetime.now(datetime.timezone.utc).isoformat(),
        "platforms": platforms,
    }
    print(json.dumps(manifest, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    sys.exit(main())
