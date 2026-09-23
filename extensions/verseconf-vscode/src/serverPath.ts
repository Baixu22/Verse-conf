import * as fs from 'fs';
import * as path from 'path';

/**
 * 语言服务器二进制在扩展包里的相对路径。
 *
 * 打包流水线把每个平台构建出的二进制放进 `server/bin/<platform>-<arch>/`，
 * 例如 `server/bin/win32-x64/verseconf-lsp.exe`、`server/bin/darwin-arm64/verseconf-lsp`。
 * 旧版本只写死了 `server/bin/verseconf-lsp.exe`，等于只支持 Windows。
 */
export function serverBinaryRelativePath(
    platform: NodeJS.Platform = process.platform,
    arch: string = process.arch
): string {
    const executable = platform === 'win32' ? 'verseconf-lsp.exe' : 'verseconf-lsp';
    return path.posix.join('server', 'bin', `${platform}-${arch}`, executable);
}

/**
 * 确保二进制在非 Windows 平台上是可执行的。
 *
 * 扩展包是 zip（.vsix），安装/解包过程不保证保留可执行位——CI 产出的
 * 跨平台二进制经常在解包后失去 `+x`。这里在启动前补一次 chmod，失败也不致命
 * （例如文件系统只读时），只是让后续 spawn 自己去报错。
 */
export function ensureExecutable(binaryPath: string, platform: NodeJS.Platform = process.platform): void {
    if (platform === 'win32') {
        return;
    }

    try {
        fs.chmodSync(binaryPath, 0o755);
    } catch {
        // 只读文件系统或权限不足：交给启动过程报错，不要在这里中断激活。
    }
}

/**
 * 在扩展目录里按平台查找语言服务器二进制。
 *
 * 查找顺序：
 *   1. `server/bin/<platform>-<arch>/verseconf-lsp[.exe]`（流水线产出的布局）
 *   2. `server/bin/verseconf-lsp[.exe]`（手工放置的兜底）
 *
 * 找不到时返回 undefined，由调用方给出可操作的提示，而不是抛异常。
 * 这个函数不依赖 `vscode`，因此可以直接用 Node 做单元测试。
 */
export function findServerBinary(
    extensionRoot: string,
    platform: NodeJS.Platform = process.platform,
    arch: string = process.arch
): string | undefined {
    const legacy = path.posix.join(
        'server',
        'bin',
        platform === 'win32' ? 'verseconf-lsp.exe' : 'verseconf-lsp'
    );

    for (const relative of [serverBinaryRelativePath(platform, arch), legacy]) {
        const absolute = path.join(extensionRoot, ...relative.split('/'));
        if (fs.existsSync(absolute)) {
            return absolute;
        }
    }

    return undefined;
}
