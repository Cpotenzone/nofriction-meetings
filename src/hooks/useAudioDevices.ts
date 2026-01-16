// noFriction Meetings - Audio Devices Hook
// Manages audio device enumeration and selection

import { useState, useEffect, useCallback } from "react";
import * as tauri from "../lib/tauri";
import type { AudioDevice, MonitorInfo } from "../lib/tauri";

export function useAudioDevices() {
    const [devices, setDevices] = useState<AudioDevice[]>([]);
    const [monitors, setMonitors] = useState<MonitorInfo[]>([]);
    const [selectedDevice, setSelectedDevice] = useState<string | null>(null);
    const [selectedMonitor, setSelectedMonitor] = useState<number | null>(null);
    const [isLoading, setIsLoading] = useState(true);
    const [error, setError] = useState<string | null>(null);

    // Load devices and monitors on mount
    useEffect(() => {
        loadDevices();
    }, []);

    const loadDevices = useCallback(async () => {
        setIsLoading(true);
        setError(null);

        try {
            const [audioDevices, availableMonitors] = await Promise.all([
                tauri.getAudioDevices(),
                tauri.getMonitors(),
            ]);

            setDevices(audioDevices);
            setMonitors(availableMonitors);

            // Set default selections
            const defaultDevice = audioDevices.find((d) => d.is_default);
            if (defaultDevice) {
                setSelectedDevice(defaultDevice.id);
            }

            const primaryMonitor = availableMonitors.find((m) => m.is_primary);
            if (primaryMonitor) {
                setSelectedMonitor(primaryMonitor.id);
            }
        } catch (err) {
            const message = err instanceof Error ? err.message : String(err);
            setError(message);
            console.error("Failed to load devices:", err);
        } finally {
            setIsLoading(false);
        }
    }, []);

    const selectDevice = useCallback(async (deviceId: string) => {
        try {
            await tauri.setAudioDevice(deviceId);
            setSelectedDevice(deviceId);
        } catch (err) {
            console.error("Failed to set audio device:", err);
            throw err;
        }
    }, []);

    const selectMonitor = useCallback(async (monitorId: number) => {
        setSelectedMonitor(monitorId);
    }, []);

    return {
        devices,
        monitors,
        selectedDevice,
        selectedMonitor,
        isLoading,
        error,
        loadDevices,
        selectDevice,
        selectMonitor,
    };
}
