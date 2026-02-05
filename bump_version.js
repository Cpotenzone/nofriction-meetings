import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

const version = process.argv[2];

if (!version) {
    console.error('Usage: node bump_version.js <new_version>');
    process.exit(1);
}

// 1. Update package.json
const packageJsonPath = path.resolve(__dirname, 'package.json');
const packageJson = JSON.parse(fs.readFileSync(packageJsonPath, 'utf-8'));
packageJson.version = version;
fs.writeFileSync(packageJsonPath, JSON.stringify(packageJson, null, 2) + '\n');
console.log(`Updated package.json to ${version}`);

// 2. Update tauri.conf.json
const tauriConfPath = path.resolve(__dirname, 'src-tauri/tauri.conf.json');
const tauriConf = JSON.parse(fs.readFileSync(tauriConfPath, 'utf-8'));
tauriConf.version = version;
fs.writeFileSync(tauriConfPath, JSON.stringify(tauriConf, null, 2) + '\n');
console.log(`Updated tauri.conf.json to ${version}`);

// 3. Update Cargo.toml
const cargoTomlPath = path.resolve(__dirname, 'src-tauri/Cargo.toml');
let cargoToml = fs.readFileSync(cargoTomlPath, 'utf-8');
// Replace version = "x.y.z"
cargoToml = cargoToml.replace(/^version = ".*"/m, `version = "${version}"`);
fs.writeFileSync(cargoTomlPath, cargoToml);
console.log(`Updated Cargo.toml to ${version}`);

// 4. Update RELEASE_VERSION if it exists, or create it
const releaseVersionPath = path.resolve(__dirname, 'RELEASE_VERSION');
fs.writeFileSync(releaseVersionPath, version);
console.log(`Updated RELEASE_VERSION to ${version}`);

console.log('Version bump complete!');
