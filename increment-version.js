import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

// Read current build number
const buildNumberPath = path.join(__dirname, 'src-tauri', 'build_number.txt');
let buildNumber = 1;

try {
    const content = fs.readFileSync(buildNumberPath, 'utf8');
    buildNumber = parseInt(content.trim(), 10) || 1;
} catch (err) {
    console.log('No build number found, starting at 1');
}

// Increment build number
buildNumber++;

// Write new build number
fs.writeFileSync(buildNumberPath, buildNumber.toString() + '\n');

// Update tauri.conf.json
const tauriConfPath = path.join(__dirname, 'src-tauri', 'tauri.conf.json');
const tauriConf = JSON.parse(fs.readFileSync(tauriConfPath, 'utf8'));

// Update version in format 1.0.0-rc.BUILD
tauriConf.version = `1.0.0-rc.${buildNumber}`;

fs.writeFileSync(tauriConfPath, JSON.stringify(tauriConf, null, 2) + '\n');

console.log(`✓ Version updated to ${tauriConf.version} (build #${buildNumber})`);
