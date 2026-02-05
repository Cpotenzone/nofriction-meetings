
import os
import re

# Logic:
# 1. Map component names to their new relative paths from 'src/'
# 2. Iterate all files in 'src/'
# 3. For each file, parse imports.
# 4. If import points to a moved component, update the path.
# 5. Also fix relative paths like '../lib' to '../../lib' if depth increased.

MAPPING = {
    "SetupWizard": "features/onboarding/SetupWizard",
    "RecordingControls": "features/capture/RecordingControls",
    "LiveTranscript": "features/capture/LiveTranscript",
    "MeetingDetectionBanner": "features/capture/MeetingDetectionBanner",
    "VideoDiagnostics": "features/capture/VideoDiagnostics",
    "MeetingHistory": "features/memory/MeetingHistory",
    "RewindTimeline": "features/memory/RewindTimeline",
    "RewindGallery": "features/memory/RewindGallery",
    "SyncedTimeline": "features/memory/SyncedTimeline",
    "AmbientTimeline": "features/memory/AmbientTimeline",
    "ActivityTimeline": "features/memory/ActivityTimeline",
    "RecordingsLibrary": "features/memory/RecordingsLibrary",
    "AIChat": "features/intelligence/AIChat",
    "CopilotPanel": "features/intelligence/CopilotPanel",
    "MeetingIntelPanel": "features/intelligence/MeetingIntelPanel",
    "LearnedDataEditor": "features/intelligence/LearnedDataEditor",
    "EntitiesView": "features/intelligence/EntitiesView",
    "PromptBrowser": "features/intelligence/PromptBrowser",
    "PromptLibrary": "features/intelligence/PromptLibrary",
    "ComparisonLab": "features/intelligence/ComparisonLab",
    "Settings": "features/settings/Settings",
    "FullSettings": "features/settings/FullSettings",
    "AISettings": "features/settings/AISettings",
    "IngestSettings": "features/settings/IngestSettings",
    "KnowledgeBaseSettings": "features/settings/KnowledgeBaseSettings",
    "PermissionsStatus": "features/settings/PermissionsStatus",
    "ThemeSelector": "features/settings/ThemeSelector",
    "TranscriptionSettings": "features/settings/TranscriptionSettings",
    "AlwaysOnSettings": "features/settings/AlwaysOnSettings",
    "ActivityThemesSettings": "features/settings/ActivityThemesSettings",
    "InsightsView": "features/analytics/InsightsView",
    "StorageMeter": "features/analytics/StorageMeter",
    "SystemStatus": "features/analytics/SystemStatus",
    "AuditLog": "features/analytics/AuditLog",
    "AdminConsole": "features/analytics/AdminConsole",
    "ToolsConsole": "features/analytics/ToolsConsole",
    "GlobalErrorBoundary": "components/common/GlobalErrorBoundary",
    "CommandPalette": "components/common/CommandPalette",
    "SearchBar": "components/common/SearchBar",
    "KBSearch": "components/common/KBSearch",
    "Sidebar": "components/layout/Sidebar",
    "Help": "components/layout/Help",
}

SRC_ROOT = os.path.abspath("src")

def get_relative_path(from_file, to_target):
    # from_file: absolute path of the file we are editing
    # to_target: relative path from src/ (e.g., "features/settings/Settings")
    
    from_dir = os.path.dirname(from_file)
    target_abs = os.path.join(SRC_ROOT, to_target)
    
    # Calculate relative path
    rel = os.path.relpath(target_abs, from_dir)
    
    # Ensure it starts with ./ or ../
    if not rel.startswith('.'):
        rel = './' + rel
        
    return rel

def fix_imports_in_file(filepath):
    with open(filepath, 'r') as f:
        content = f.read()
    
    # Check if this file ITSELF is one of the moved files (to fix ../lib etc)
    # Most moved files went from src/components/ (depth 2) to src/features/xxx/ (depth 3)
    # So ../lib becomes ../../lib
    
    rel_path_from_src = os.path.relpath(filepath, SRC_ROOT)
    parts = rel_path_from_src.split(os.sep)
    
    is_moved_feature_file = parts[0] == "features" and len(parts) >= 3 # src/features/name/File.tsx
    is_moved_layout_file = parts[0] == "components" and (parts[1] == "layout" or parts[1] == "common") # src/components/layout/File.tsx
    
    new_content = content
    
    # 1. Fix generic imports (../lib, ../hooks, etc) ONLY for moved files
    # We assume they used to be strict ../lib or ../hooks from src/components/
    if (is_moved_feature_file or is_moved_layout_file):
        # Replacements for depth change
        # ../lib -> ../../lib
        # ../hooks -> ../../hooks
        # ../contexts -> ../../contexts
        # ../assets -> ../../assets
        # ../utils -> ../../utils
        # BUT be careful not to double replace if we run this script twice.
        # We can check if it already looks like ../../
        
        # Regex lookbehind to ensure we don't match ../../
        # negative lookbehind is hard in python regex for variable length, but fixed length is fine.
        # (?<!\.\./)\.\./lib -> match ../lib but not ../../lib
        
        for folder in ["lib", "hooks", "contexts", "assets", "utils", "types"]:
            pattern = re.compile(r'(?<!\.\./)\.\./' + folder)
            new_content = pattern.sub(f'../../{folder}', new_content)

    # 2. Fix component imports
    # Look for import ... from "..."
    # We need to capture the import path.
    
    def replace_import(match):
        full_match = match.group(0) # import ... from "..."
        quote = match.group(2) # " or '
        path_str = match.group(3) # ./components/Settings or ../Settings
        
        # Check if path_str refers to one of our mapped components
        # We need to resolve what it's pointing to first.
        
        # Construct absolute path of the import target
        try:
            current_dir = os.path.dirname(filepath)
            # If path doesn't start with ., it's a node_module, ignore
            if not path_str.startswith('.'):
                return full_match
                
            # If it's a CSS module, ignore for now (or handle if moved)
            if path_str.endswith('.css'):
                 # We did move VideoDiagnostics.module.css
                 if "VideoDiagnostics.module.css" in path_str and "VideoDiagnostics" in filepath:
                     # Sibling import, stays ./VideoDiagnostics.module.css
                     return full_match
                 return full_match

            # Resolve absolute path (this might be tricky if the file doesn't exist anymore at old location)
            # But we know the NAME of the component.
            # Heuristic: search for the component name in the path string.
            
            basename = os.path.basename(path_str)
            # Remove extension if present (imports usually don't have it)
            if basename.endswith('.tsx') or basename.endswith('.ts'):
                basename = os.path.splitext(basename)[0]
                
            if basename in MAPPING:
                # It is one of our moved files!
                # Calculate new relative path
                new_rel = get_relative_path(filepath, MAPPING[basename])
                # Preserve quote style
                return match.group(1) + quote + new_rel + quote + match.group(4)
            
            # Special case: importing from "components/..." (old structure)
            if "components/" in path_str:
                # Try to see if the filename matches
                possible_name = os.path.basename(path_str)
                if possible_name in MAPPING:
                    new_rel = get_relative_path(filepath, MAPPING[possible_name])
                    return match.group(1) + quote + new_rel + quote + match.group(4)
                    
        except Exception as e:
            print(f"Error processing import {path_str} in {filepath}: {e}")
            return full_match
            
        return full_match

    # Regex to find imports: import ... from "PATH"; or import "PATH";
    # Group 1: everything before quote
    # Group 2: Quote char
    # Group 3: Path
    # Group 4: End quote + rest
    import_pattern = re.compile(r'(import\s+.*?from\s+)(["\'])(.*?)(["\'];?)', re.DOTALL)
    new_content = import_pattern.sub(replace_import, new_content)
    
    # Also handle dynamic imports: import("...")
    dynamic_import_pattern = re.compile(r'(import\()(["\'])(.*?)(["\']\))')
    new_content = dynamic_import_pattern.sub(replace_import, new_content)

    if new_content != content:
        print(f"Updating {filepath}")
        with open(filepath, 'w') as f:
            f.write(new_content)

def main():
    print("Starting import fix...")
    for root, dirs, files in os.walk(SRC_ROOT):
        for file in files:
            if file.endswith('.tsx') or file.endswith('.ts'):
                fix_imports_in_file(os.path.join(root, file))
    print("Done.")

if __name__ == "__main__":
    main()
