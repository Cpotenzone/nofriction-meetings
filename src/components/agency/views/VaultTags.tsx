import React, { useState, useEffect } from 'react';
import { Hash } from 'lucide-react';
import * as tauri from '../../../lib/tauri';
import { VaultTag } from '../../../lib/tauri';
import './VaultTags.css';

interface VaultTagsProps {
    onSelectTag: (tag: string) => void;
    activeTag: string | null;
}

export const VaultTags: React.FC<VaultTagsProps> = ({ onSelectTag, activeTag }) => {
    const [tags, setTags] = useState<VaultTag[]>([]);
    const [isLoading, setIsLoading] = useState(false);

    useEffect(() => {
        loadTags();
    }, []);

    const loadTags = async () => {
        setIsLoading(true);
        try {
            const vaultTags = await tauri.listVaultTags();
            // Sort by count descending
            vaultTags.sort((a, b) => b.fileCount - a.fileCount);
            setTags(vaultTags);
        } catch (err) {
            console.error("Failed to load tags:", err);
        } finally {
            setIsLoading(false);
        }
    };

    if (isLoading) {
        return <div className="tags-loading">Loading tags...</div>;
    }

    if (tags.length === 0) {
        return null;
    }

    return (
        <div className="vault-tags-section">
            <h3><Hash size={14} /> Tags</h3>
            <div className="tags-list">
                {tags.map(tag => (
                    <div
                        key={tag.name}
                        className={`tag-item ${activeTag === tag.name ? 'active' : ''}`}
                        onClick={() => onSelectTag(tag.name)}
                    >
                        <span className="tag-name">#{tag.name}</span>
                        <span className="tag-count">{tag.fileCount}</span>
                    </div>
                ))}
            </div>
        </div>
    );
};
