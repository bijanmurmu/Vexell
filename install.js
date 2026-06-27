const fs = require('fs');
const path = require('path');
const https = require('https');
const { execSync } = require('child_process');

const version = require('./package.json').version;
const platform = process.platform;

let binName = '';
if (platform === 'win32') {
    binName = 'vexell-windows-x64.exe';
} else if (platform === 'darwin') {
    binName = 'vexell-macos-x64';
} else if (platform === 'linux') {
    binName = 'vexell-linux-x64';
} else {
    console.error(`❌ Unsupported platform: ${platform}`);
    process.exit(1);
}

const url = `https://github.com/bijanmurmu/Vexell/releases/download/v${version}/${binName}`;
const binDir = path.join(__dirname, 'bin');
if (!fs.existsSync(binDir)) {
    fs.mkdirSync(binDir);
}

const destPath = path.join(binDir, platform === 'win32' ? 'vexell.exe' : 'vexell');

console.log(`\n🚀 Downloading Vexell Rust engine v${version} for ${platform}...`);

function downloadFile(url, dest) {
    return new Promise((resolve, reject) => {
        const file = fs.createWriteStream(dest);
        https.get(url, (response) => {
            if (response.statusCode === 301 || response.statusCode === 302) {
                downloadFile(response.headers.location, dest).then(resolve).catch(reject);
            } else if (response.statusCode === 200) {
                response.pipe(file);
                file.on('finish', () => {
                    file.close();
                    if (platform !== 'win32') {
                        execSync(`chmod +x "${dest}"`);
                    }
                    console.log("✅ Vexell successfully installed!\n");
                    resolve();
                });
            } else {
                reject(new Error(`Failed to download binary: HTTP ${response.statusCode}`));
            }
        }).on('error', (err) => {
            fs.unlink(dest, () => {});
            reject(err);
        });
    });
}

downloadFile(url, destPath).catch(err => {
    console.error(`❌ Installation failed: ${err.message}`);
    process.exit(1);
});
