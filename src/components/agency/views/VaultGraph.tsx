import React, { useState, useEffect, useRef } from 'react';
import ForceGraph2D from 'react-force-graph-2d';
import { X } from 'lucide-react';
import * as tauri from '../../../lib/tauri';
import { VaultGraph as VaultGraphData } from '../../../lib/tauri';
import './VaultGraph.css';

interface VaultGraphProps {
    onClose: () => void;
    onSelectNode: (path: string) => void;
}

export const VaultGraph: React.FC<VaultGraphProps> = ({ onClose, onSelectNode }) => {
    const [graphData, setGraphData] = useState<VaultGraphData>({ nodes: [], edges: [] });
    const [isLoading, setIsLoading] = useState(true);
    const containerRef = useRef<HTMLDivElement>(null);
    const [dimensions, setDimensions] = useState({ width: 800, height: 600 });

    const [hoverNode, setHoverNode] = useState<string | null>(null);
    const [highlightNodes, setHighlightNodes] = useState(new Set());
    const [highlightLinks, setHighlightLinks] = useState(new Set());

    useEffect(() => {
        loadGraphData();
        updateDimensions();
        window.addEventListener('resize', updateDimensions);
        return () => window.removeEventListener('resize', updateDimensions);
    }, []);

    const updateDimensions = () => {
        if (containerRef.current) {
            setDimensions({
                width: containerRef.current.clientWidth,
                height: containerRef.current.clientHeight
            });
        }
    };

    const loadGraphData = async () => {
        setIsLoading(true);
        try {
            const data = await tauri.getVaultGraph();
            setGraphData(data);
        } catch (err) {
            console.error("Failed to load vault graph:", err);
        } finally {
            setIsLoading(false);
        }
    };

    // Formatter for ForceGraph2D
    const data = {
        nodes: graphData.nodes.map(n => ({
            id: n.id,
            name: n.label,
            val: n.fileType === 'meeting' ? 6 : 4,
            color: n.fileType === 'meeting' ? '#ffd700' : '#4a9eff',
            type: n.fileType
        })),
        links: graphData.edges.map(e => ({
            source: e.source,
            target: e.target
        }))
    };

    const updateHighlight = () => {
        setHighlightNodes(new Set(highlightNodes));
        setHighlightLinks(new Set(highlightLinks));
    };

    const handleNodeHover = (node: any) => {
        highlightNodes.clear();
        highlightLinks.clear();
        if (node) {
            highlightNodes.add(node.id);
            graphData.edges.forEach(link => {
                if (link.source === node.id || link.target === node.id) {
                    highlightLinks.add(`${link.source}-${link.target}`);
                    highlightNodes.add(link.source);
                    highlightNodes.add(link.target);
                }
            });
        }
        setHoverNode(node ? node.id : null); // Store node.id instead of node object
        updateHighlight();
    };

    return (
        <div className="vault-graph-overlay" ref={containerRef}>
            <div className="graph-header">
                <h3>Vault Knowledge Graph</h3>
                <div className="graph-controls">
                    <button onClick={loadGraphData} title="Refresh Graph">
                        Refresh
                    </button>
                    <button className="close-btn" onClick={onClose}>
                        <X size={20} />
                    </button>
                </div>
            </div>

            {isLoading ? (
                <div className="graph-loading">
                    <div className="loading-spinner" />
                    <p>Building knowledge map...</p>
                </div>
            ) : (
                <ForceGraph2D
                    graphData={data}
                    width={dimensions.width}
                    height={dimensions.height}
                    nodeLabel="name"
                    nodeCanvasObject={(node: any, ctx, globalScale) => {
                        const label = node.name;
                        const fontSize = 12 / globalScale;
                        ctx.font = `${fontSize}px Inter, sans-serif`;

                        const isHighlighted = highlightNodes.has(node.id);
                        const radius = node.type === 'meeting' ? 5 : 3;

                        // Draw outer glow if highlighted
                        if (isHighlighted || node.id === hoverNode) {
                            ctx.beginPath();
                            ctx.arc(node.x, node.y, radius * 1.8, 0, 2 * Math.PI, false);
                            ctx.fillStyle = node.color + '33';
                            ctx.fill();
                        }

                        // Draw main node
                        ctx.beginPath();
                        ctx.arc(node.x, node.y, radius, 0, 2 * Math.PI, false);
                        ctx.fillStyle = node.color;
                        ctx.shadowBlur = isHighlighted ? 15 : 5;
                        ctx.shadowColor = node.color;
                        ctx.fill();

                        // Reset shadow
                        ctx.shadowBlur = 0;

                        // Draw label if zoomed in or highlighted
                        if (globalScale > 1.5 || isHighlighted) {
                            const textWidth = ctx.measureText(label).width;
                            const bckgDimensions = [textWidth, fontSize].map(n => n + fontSize * 0.2);

                            ctx.fillStyle = 'rgba(0, 0, 0, 0.6)';
                            ctx.fillRect(node.x - bckgDimensions[0] / 2, node.y + radius + 2, bckgDimensions[0], bckgDimensions[1]);

                            ctx.textAlign = 'center';
                            ctx.textBaseline = 'top';
                            ctx.fillStyle = isHighlighted ? '#fff' : 'rgba(255, 255, 255, 0.6)';
                            ctx.fillText(label, node.x, node.y + radius + 2);
                        }
                    }}
                    nodePointerAreaPaint={(node: any, color, ctx) => {
                        ctx.fillStyle = color;
                        const radius = node.type === 'meeting' ? 8 : 6;
                        ctx.beginPath(); ctx.arc(node.x, node.y, radius, 0, 2 * Math.PI, false); ctx.fill();
                    }}
                    linkColor={link => highlightLinks.has(`${(link.source as any).id}-${(link.target as any).id}`) ? '#FFB800' : 'rgba(255, 255, 255, 0.08)'}
                    linkWidth={link => highlightLinks.has(`${(link.source as any).id}-${(link.target as any).id}`) ? 2 : 1}
                    linkDirectionalParticles={2}
                    linkDirectionalParticleWidth={link => highlightLinks.has(`${(link.source as any).id}-${(link.target as any).id}`) ? 4 : 0}
                    onNodeHover={handleNodeHover}
                    onNodeClick={(node: any) => {
                        onSelectNode(node.id);
                        onClose();
                    }}
                    backgroundColor="#000000"
                    d3AlphaDecay={0.02}
                    d3VelocityDecay={0.3}
                />
            )}

            <div className="graph-legend">
                <div className="legend-item">
                    <span className="dot meeting"></span>
                    <span>Meeting</span>
                </div>
                <div className="legend-item">
                    <span className="dot note"></span>
                    <span>Note</span>
                </div>
            </div>
        </div>
    );
};
