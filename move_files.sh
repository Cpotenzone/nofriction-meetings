#!/bin/bash
mkdir -p src/features/intelligence
mkdir -p src/features/capture
mkdir -p src/features/memory
mkdir -p src/features/analytics
mkdir -p src/components/common
mkdir -p src/components/layout
mkdir -p src/features/settings

# Force move files to avoid prompts
mv -f src/components/AIChat.tsx src/features/intelligence/ 2>/dev/null
mv -f src/components/CopilotPanel.tsx src/features/intelligence/ 2>/dev/null
mv -f src/components/MeetingIntelPanel.tsx src/features/intelligence/ 2>/dev/null
mv -f src/components/LearnedDataEditor.tsx src/features/intelligence/ 2>/dev/null
mv -f src/components/EntitiesView.tsx src/features/intelligence/ 2>/dev/null
mv -f src/components/PromptBrowser.tsx src/features/intelligence/ 2>/dev/null
mv -f src/components/PromptLibrary.tsx src/features/intelligence/ 2>/dev/null
mv -f src/components/ComparisonLab.tsx src/features/intelligence/ 2>/dev/null

mv -f src/components/RecordingControls.tsx src/features/capture/ 2>/dev/null
mv -f src/components/LiveTranscript.tsx src/features/capture/ 2>/dev/null
mv -f src/components/MeetingDetectionBanner.tsx src/features/capture/ 2>/dev/null
mv -f src/components/VideoDiagnostics.tsx src/features/capture/ 2>/dev/null
mv -f src/components/VideoDiagnostics.module.css src/features/capture/ 2>/dev/null

mv -f src/components/MeetingHistory.tsx src/features/memory/ 2>/dev/null
mv -f src/components/RewindTimeline.tsx src/features/memory/ 2>/dev/null
mv -f src/components/RewindGallery.tsx src/features/memory/ 2>/dev/null
mv -f src/components/SyncedTimeline.tsx src/features/memory/ 2>/dev/null
mv -f src/components/AmbientTimeline.tsx src/features/memory/ 2>/dev/null
mv -f src/components/ActivityTimeline.tsx src/features/memory/ 2>/dev/null
mv -f src/components/RecordingsLibrary.tsx src/features/memory/ 2>/dev/null

mv -f src/components/InsightsView.tsx src/features/analytics/ 2>/dev/null
mv -f src/components/StorageMeter.tsx src/features/analytics/ 2>/dev/null
mv -f src/components/SystemStatus.tsx src/features/analytics/ 2>/dev/null
mv -f src/components/AuditLog.tsx src/features/analytics/ 2>/dev/null
mv -f src/components/AdminConsole.tsx src/features/analytics/ 2>/dev/null
mv -f src/components/ToolsConsole.tsx src/features/analytics/ 2>/dev/null

mv -f src/components/GlobalErrorBoundary.tsx src/components/common/ 2>/dev/null
mv -f src/components/CommandPalette.tsx src/components/common/ 2>/dev/null
mv -f src/components/SearchBar.tsx src/components/common/ 2>/dev/null
mv -f src/components/KBSearch.tsx src/components/common/ 2>/dev/null

mv -f src/components/Sidebar.tsx src/components/layout/ 2>/dev/null
mv -f src/components/Help.tsx src/components/layout/ 2>/dev/null
