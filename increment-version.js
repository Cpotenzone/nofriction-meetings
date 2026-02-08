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

// Read tauri.conf.json to get current version
const tauriConfPath = path.join(__dirname, 'src-tauri', 'tauri.conf.json');
const tauriConf = JSON.parse(fs.readFileSync(tauriConfPath, 'utf8'));

// Parse current version - preserve major.minor.patch if it's not an RC version
const currentVersion = tauriConf.version;
let newVersion;

// If version is 1.5.0 or higher (not an RC), keep it as-is
if (currentVersion && !currentVersion.includes('-rc.') && (currentVersion.match(/^[2-9]\./) || currentVersion.startsWith('1.5.'))) {
    newVersion = currentVersion;
    console.log(`✓ Preserving version ${newVersion} (build #${buildNumber})`);
} else {
    // Default behavior for RC versions
    newVersion = `1.0.0-rc.${buildNumber}`;
    tauriConf.version = newVersion;
    fs.writeFileSync(tauriConfPath, JSON.stringify(tauriConf, null, 2) + '\n');
    console.log(`✓ Version updated to ${newVersion} (build #${buildNumber})`);
}
