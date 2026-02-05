import { useState, useEffect } from 'react';
import { getVersion } from '@tauri-apps/api/app';

export const useAppVersion = () => {
    const [version, setVersion] = useState<string>('');

    useEffect(() => {
        const fetchVersion = async () => {
            try {
                const v = await getVersion();
                setVersion(v);
            } catch (err) {
                console.error('Failed to get app version:', err);
                setVersion('Unknown');
            }
        };

        fetchVersion();
    }, []);

    return version;
};
