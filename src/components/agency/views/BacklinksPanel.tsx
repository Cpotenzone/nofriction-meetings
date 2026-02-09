import React, { useState, useEffect } from 'react';
import { Link } from 'lucide-react';
import * as tauri from '../../../lib/tauri';
import { BacklinkResult } from '../../../lib/tauri';
import './BacklinksPanel.css';

interface BacklinksPanelProps {
    filePath: string | null;
    onNavigate: (path: string) => void;
}

export const BacklinksPanel: React.FC<BacklinksPanelProps> = ({ filePath, onNavigate }) => {
    const [backlinks, setBacklinks] = useState<BacklinkResult | null>(null);
    const [isLoading, setIsLoading] = useState(false);

    useEffect(() => {
        if (filePath) {
            loadBacklinks(filePath);
        } else {
            setBacklinks(null);
        }
    }, [filePath]);

    const loadBacklinks = async (path: string) => {
        setIsLoading(true);
        try {
            const result = await tauri.getVaultBacklinks(path);
            setBacklinks(result);
        } catch (err) {
            console.error("Failed to load backlinks:", err);
            setBacklinks(null);
        } finally {
            setIsLoading(false);
        }
    };

    if (!filePath) {
        return <div className="backlinks-empty">Select a file to see backlinks</div>;
    }

    if (isLoading) {
        return <div className="backlinks-loading">Finding connections...</div>;
    }

    if (!backlinks || backlinks.backlinks.length === 0) {
        return <div className="backlinks-empty">No backlinks found</div>;
    }

    return (
        <div className="backlinks-panel">
            <h4><Link size={14} /> Backlinks ({backlinks.backlinks.length})</h4>
            <div className="backlinks-list">
                {backlinks.backlinks.map((link, index) => (
                    <div
                        key={`${link.sourceFile}-${index}`}
                        className="backlink-item"
                        onClick={() => onNavigate(link.sourceFile)}
                    >
                        <div className="backlink-header">
                            <span className="source-file">{link.sourceFile.split('/').pop()?.replace('.md', '')}</span>
                            <span className="line-num">:{link.lineNumber}</span>
                        </div>
                        <div className="backlink-context">
                            {link.displayText || "Link"}
                        </div>
                    </div>
                ))}
            </div>
        </div>
    );
};
