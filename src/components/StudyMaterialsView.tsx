// noFriction Meetings - Study Materials View
// Displays AI-generated study materials after a Dork Mode session

import { useState } from "react";

interface KeyConcept {
    term: string;
    definition: string;
}

interface QuizQuestion {
    question: string;
    options: string[];
    correct_index: number;
    explanation: string;
}

interface StudyMaterials {
    session_id: string;
    summary: string;
    concepts: KeyConcept[];
    quiz: QuizQuestion[];
    created_at: string;
}

interface StudyMaterialsViewProps {
    materials: StudyMaterials;
    onClose?: () => void;
}

export function StudyMaterialsView({ materials, onClose }: StudyMaterialsViewProps) {
    const [activeTab, setActiveTab] = useState<"summary" | "concepts" | "quiz">("summary");
    const [quizAnswers, setQuizAnswers] = useState<Record<number, number>>({});
    const [showResults, setShowResults] = useState(false);

    const handleQuizAnswer = (questionIndex: number, optionIndex: number) => {
        if (showResults) return;
        setQuizAnswers(prev => ({ ...prev, [questionIndex]: optionIndex }));
    };

    const checkQuiz = () => {
        setShowResults(true);
    };

    const resetQuiz = () => {
        setQuizAnswers({});
        setShowResults(false);
    };

    const getScore = () => {
        let correct = 0;
        materials.quiz.forEach((q, i) => {
            if (quizAnswers[i] === q.correct_index) correct++;
        });
        return correct;
    };

    const copyToClipboard = (text: string) => {
        navigator.clipboard.writeText(text);
    };

    return (
        <div className="study-materials-view" style={{
            background: "var(--bg-primary)",
            borderRadius: "16px",
            padding: "24px",
            minHeight: "500px",
        }}>
            {/* Header */}
            <div style={{
                display: "flex",
                justifyContent: "space-between",
                alignItems: "center",
                marginBottom: "24px",
            }}>
                <div style={{ display: "flex", alignItems: "center", gap: "12px" }}>
                    <span style={{ fontSize: "2rem" }}>📚</span>
                    <div>
                        <h2 style={{ margin: 0, fontSize: "1.5rem" }}>Study Materials</h2>
                        <div style={{ color: "var(--text-muted)", fontSize: "0.85rem" }}>
                            Session: {materials.session_id.slice(0, 8)}...
                        </div>
                    </div>
                </div>
                {onClose && (
                    <button
                        onClick={onClose}
                        style={{
                            background: "var(--bg-secondary)",
                            border: "1px solid var(--border)",
                            borderRadius: "8px",
                            padding: "8px 16px",
                            cursor: "pointer",
                            color: "var(--text-primary)",
                        }}
                    >
                        ✕ Close
                    </button>
                )}
            </div>

            {/* Tabs */}
            <div style={{
                display: "flex",
                gap: "8px",
                marginBottom: "24px",
                borderBottom: "1px solid var(--border)",
                paddingBottom: "16px",
            }}>
                {[
                    { id: "summary", label: "📝 Summary", count: null },
                    { id: "concepts", label: "💡 Key Concepts", count: materials.concepts.length },
                    { id: "quiz", label: "❓ Quiz", count: materials.quiz.length },
                ].map(tab => (
                    <button
                        key={tab.id}
                        onClick={() => setActiveTab(tab.id as typeof activeTab)}
                        style={{
                            padding: "10px 20px",
                            borderRadius: "8px",
                            border: "none",
                            background: activeTab === tab.id
                                ? "linear-gradient(135deg, #9333ea, #4f46e5)"
                                : "var(--bg-secondary)",
                            color: activeTab === tab.id ? "#fff" : "var(--text-primary)",
                            cursor: "pointer",
                            fontWeight: 500,
                            transition: "all 0.2s ease",
                        }}
                    >
                        {tab.label}
                        {tab.count !== null && (
                            <span style={{
                                marginLeft: "8px",
                                background: "rgba(255,255,255,0.2)",
                                padding: "2px 8px",
                                borderRadius: "12px",
                                fontSize: "0.8rem",
                            }}>
                                {tab.count}
                            </span>
                        )}
                    </button>
                ))}
            </div>

            {/* Content */}
            <div style={{ minHeight: "300px" }}>
                {/* Summary Tab */}
                {activeTab === "summary" && (
                    <div style={{
                        background: "var(--bg-secondary)",
                        borderRadius: "12px",
                        padding: "20px",
                    }}>
                        <div style={{
                            display: "flex",
                            justifyContent: "space-between",
                            alignItems: "center",
                            marginBottom: "16px",
                        }}>
                            <h3 style={{ margin: 0 }}>Session Summary</h3>
                            <button
                                onClick={() => copyToClipboard(materials.summary)}
                                style={{
                                    background: "transparent",
                                    border: "1px solid var(--border)",
                                    borderRadius: "6px",
                                    padding: "6px 12px",
                                    cursor: "pointer",
                                    fontSize: "0.85rem",
                                    color: "var(--text-muted)",
                                }}
                            >
                                📋 Copy
                            </button>
                        </div>
                        <div style={{
                            lineHeight: 1.7,
                            color: "var(--text-primary)",
                            whiteSpace: "pre-wrap",
                        }}>
                            {materials.summary}
                        </div>
                    </div>
                )}

                {/* Concepts Tab */}
                {activeTab === "concepts" && (
                    <div style={{ display: "flex", flexDirection: "column", gap: "12px" }}>
                        {materials.concepts.map((concept, i) => (
                            <div key={i} style={{
                                background: "var(--bg-secondary)",
                                borderRadius: "12px",
                                padding: "16px 20px",
                                borderLeft: "4px solid #9333ea",
                            }}>
                                <div style={{
                                    fontWeight: 600,
                                    fontSize: "1.1rem",
                                    marginBottom: "8px",
                                    color: "var(--accent)",
                                }}>
                                    {concept.term}
                                </div>
                                <div style={{ color: "var(--text-primary)", lineHeight: 1.6 }}>
                                    {concept.definition}
                                </div>
                            </div>
                        ))}
                        {materials.concepts.length === 0 && (
                            <div style={{ textAlign: "center", color: "var(--text-muted)", padding: "40px" }}>
                                No key concepts extracted from this session.
                            </div>
                        )}
                    </div>
                )}

                {/* Quiz Tab */}
                {activeTab === "quiz" && (
                    <div>
                        {materials.quiz.map((q, qIndex) => (
                            <div key={qIndex} style={{
                                background: "var(--bg-secondary)",
                                borderRadius: "12px",
                                padding: "20px",
                                marginBottom: "16px",
                            }}>
                                <div style={{
                                    fontWeight: 600,
                                    marginBottom: "16px",
                                    display: "flex",
                                    gap: "8px",
                                }}>
                                    <span style={{
                                        background: "linear-gradient(135deg, #9333ea, #4f46e5)",
                                        color: "#fff",
                                        borderRadius: "50%",
                                        width: "28px",
                                        height: "28px",
                                        display: "flex",
                                        alignItems: "center",
                                        justifyContent: "center",
                                        fontSize: "0.9rem",
                                        flexShrink: 0,
                                    }}>
                                        {qIndex + 1}
                                    </span>
                                    <span>{q.question}</span>
                                </div>
                                <div style={{ display: "flex", flexDirection: "column", gap: "8px" }}>
                                    {q.options.map((option, oIndex) => {
                                        const isSelected = quizAnswers[qIndex] === oIndex;
                                        const isCorrect = q.correct_index === oIndex;

                                        return (
                                            <button
                                                key={oIndex}
                                                onClick={() => handleQuizAnswer(qIndex, oIndex)}
                                                style={{
                                                    padding: "12px 16px",
                                                    borderRadius: "8px",
                                                    border: isSelected
                                                        ? "2px solid var(--accent)"
                                                        : "1px solid var(--border)",
                                                    background: showResults
                                                        ? isCorrect
                                                            ? "rgba(34, 197, 94, 0.2)"
                                                            : isSelected
                                                                ? "rgba(239, 68, 68, 0.2)"
                                                                : "transparent"
                                                        : isSelected
                                                            ? "rgba(147, 51, 234, 0.1)"
                                                            : "transparent",
                                                    color: "var(--text-primary)",
                                                    cursor: showResults ? "default" : "pointer",
                                                    textAlign: "left",
                                                    transition: "all 0.2s ease",
                                                }}
                                            >
                                                <span style={{ marginRight: "8px" }}>
                                                    {String.fromCharCode(65 + oIndex)}.
                                                </span>
                                                {option}
                                                {showResults && isCorrect && " ✓"}
                                            </button>
                                        );
                                    })}
                                </div>
                                {showResults && quizAnswers[qIndex] !== undefined && (
                                    <div style={{
                                        marginTop: "12px",
                                        padding: "12px",
                                        background: "rgba(147, 51, 234, 0.1)",
                                        borderRadius: "8px",
                                        fontSize: "0.9rem",
                                        color: "var(--text-muted)",
                                    }}>
                                        💡 {q.explanation}
                                    </div>
                                )}
                            </div>
                        ))}

                        {materials.quiz.length > 0 && (
                            <div style={{
                                display: "flex",
                                justifyContent: "space-between",
                                alignItems: "center",
                                marginTop: "20px",
                            }}>
                                {showResults ? (
                                    <>
                                        <div style={{
                                            fontSize: "1.2rem",
                                            fontWeight: 600,
                                            color: getScore() === materials.quiz.length
                                                ? "#22c55e"
                                                : "var(--text-primary)",
                                        }}>
                                            Score: {getScore()} / {materials.quiz.length}
                                            {getScore() === materials.quiz.length && " 🎉"}
                                        </div>
                                        <button
                                            onClick={resetQuiz}
                                            style={{
                                                background: "var(--bg-secondary)",
                                                border: "1px solid var(--border)",
                                                borderRadius: "8px",
                                                padding: "10px 20px",
                                                cursor: "pointer",
                                                color: "var(--text-primary)",
                                            }}
                                        >
                                            🔄 Retry Quiz
                                        </button>
                                    </>
                                ) : (
                                    <button
                                        onClick={checkQuiz}
                                        disabled={Object.keys(quizAnswers).length < materials.quiz.length}
                                        style={{
                                            background: Object.keys(quizAnswers).length < materials.quiz.length
                                                ? "var(--bg-tertiary)"
                                                : "linear-gradient(135deg, #9333ea, #4f46e5)",
                                            border: "none",
                                            borderRadius: "8px",
                                            padding: "12px 24px",
                                            cursor: Object.keys(quizAnswers).length < materials.quiz.length
                                                ? "not-allowed"
                                                : "pointer",
                                            color: "#fff",
                                            fontWeight: 600,
                                            width: "100%",
                                        }}
                                    >
                                        ✓ Check Answers ({Object.keys(quizAnswers).length}/{materials.quiz.length})
                                    </button>
                                )}
                            </div>
                        )}

                        {materials.quiz.length === 0 && (
                            <div style={{ textAlign: "center", color: "var(--text-muted)", padding: "40px" }}>
                                No quiz questions generated for this session.
                            </div>
                        )}
                    </div>
                )}
            </div>
        </div>
    );
}
