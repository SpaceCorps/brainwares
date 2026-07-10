import React, { useState, useEffect, useRef } from 'react';
import { marked } from 'marked';
import { 
  BookOpen, Search, ShieldCheck, AlertCircle, RefreshCw, 
  Tag, Link2, Share2, Compass, Network, FileCode, CheckCircle 
} from 'lucide-react';
import data from './data.json';

const renderer = new marked.Renderer();
const originalLink = renderer.link.bind(renderer);
renderer.link = (href, title, text) => {
  if (href.startsWith('wiki:')) {
    const noteName = href.replace('wiki:', '');
    return `<a href="#" class="wiki-link text-indigo-400 hover:text-indigo-300 font-semibold underline decoration-indigo-500/40" data-note="${noteName}">${text}</a>`;
  }
  return originalLink(href, title, text);
};
marked.setOptions({ renderer });

export default function App() {
  const [memories, setMemories] = useState(data.memories || []);
  const [selectedNoteName, setSelectedNoteName] = useState('index');
  const [searchQuery, setSearchQuery] = useState('');
  const [selectedTag, setSelectedTag] = useState(null);
  const [viewMode, setViewMode] = useState('doc');
  const canvasRef = useRef(null);

  const preprocessMarkdown = (text) => {
    return text.replace(/\[\[(.*?)\]\]/g, (match, note) => {
      const parts = note.split('|');
      const target = parts[0].trim();
      const label = parts[1] ? parts[1].trim() : target;
      const normTarget = target.toLowerCase().replace(/ /g, '-').replace(/_/g, '-');
      return `[${label}](wiki:${normTarget})`;
    });
  };

  const selectedNote = memories.find(m => m.name.toLowerCase() === selectedNoteName.toLowerCase()) 
    || memories.find(m => m.name.toLowerCase() === 'index') 
    || memories[0];

  useEffect(() => {
    if (selectedNote) {
      setSelectedNoteName(selectedNote.name);
    }
  }, [selectedNote]);

  const handleHtmlClick = (e) => {
    const target = e.target.closest('[data-note]');
    if (target) {
      e.preventDefault();
      const noteName = target.getAttribute('data-note');
      setSelectedNoteName(noteName);
    }
  };

  const allTags = Array.from(new Set(memories.flatMap(m => m.frontmatter.tags || [])));

  const filteredNotes = memories.filter(m => {
    const matchesSearch = searchQuery === '' || 
      m.name.toLowerCase().includes(searchQuery.toLowerCase()) ||
      (m.frontmatter.title || '').toLowerCase().includes(searchQuery.toLowerCase()) ||
      m.body.toLowerCase().includes(searchQuery.toLowerCase());
    
    const matchesTag = !selectedTag || (m.frontmatter.tags || []).includes(selectedTag);
    return matchesSearch && matchesTag;
  });

  useEffect(() => {
    if (viewMode !== 'graph' || !canvasRef.current) return;
    const canvas = canvasRef.current;
    const ctx = canvas.getContext('2d');
    
    const resizeCanvas = () => {
      canvas.width = canvas.parentElement.clientWidth;
      canvas.height = canvas.parentElement.clientHeight || 500;
    };
    resizeCanvas();
    window.addEventListener('resize', resizeCanvas);

    const nodes = memories.map((m, index) => {
      const angle = (index / memories.length) * Math.PI * 2;
      const radius = Math.min(canvas.width, canvas.height) * 0.3;
      return {
        id: m.name.toLowerCase(),
        name: m.frontmatter.title || m.name,
        x: canvas.width / 2 + Math.cos(angle) * radius,
        y: canvas.height / 2 + Math.sin(angle) * radius,
        vx: 0,
        vy: 0,
        radius: m.name.toLowerCase() === selectedNoteName.toLowerCase() ? 12 : 8,
        isGlobal: m.file_path.includes('.config'),
      };
    });

    const edges = [];
    memories.forEach(m => {
      const matches = m.body.match(/\[\[(.*?)\]\]/g) || [];
      matches.forEach(match => {
        const target = match.replace(/\[\[|\]\]/g, '').split('|')[0].trim()
          .toLowerCase().replace(/ /g, '-').replace(/_/g, '-');
        
        if (nodes.some(n => n.id === target)) {
          edges.push({
            source: nodes.find(n => n.id === m.name.toLowerCase()),
            target: nodes.find(n => n.id === target),
          });
        }
      });
    });

    let animationId;
    let draggedNode = null;

    const step = () => {
      for (let i = 0; i < nodes.length; i++) {
        for (let j = i + 1; j < nodes.length; j++) {
          const n1 = nodes[i];
          const n2 = nodes[j];
          const dx = n2.x - n1.x;
          const dy = n2.y - n1.y;
          const dist = Math.sqrt(dx * dx + dy * dy) || 1;
          if (dist < 200) {
            const force = (200 - dist) * 0.08;
            const fx = (dx / dist) * force;
            const fy = (dy / dist) * force;
            if (n1 !== draggedNode) { n1.vx -= fx; n1.vy -= fy; }
            if (n2 !== draggedNode) { n2.vx += fx; n2.vy += fy; }
          }
        }
      }

      edges.forEach(edge => {
        const n1 = edge.source;
        const n2 = edge.target;
        const dx = n2.x - n1.x;
        const dy = n2.y - n1.y;
        const dist = Math.sqrt(dx * dx + dy * dy) || 1;
        const force = dist * 0.02;
        const fx = (dx / dist) * force;
        const fy = (dy / dist) * force;
        if (n1 !== draggedNode) { n1.vx += fx; n1.vy += fy; }
        if (n2 !== draggedNode) { n2.vx -= fx; n2.vy -= fy; }
      });

      const cx = canvas.width / 2;
      const cy = canvas.height / 2;
      nodes.forEach(node => {
        if (node === draggedNode) return;
        const dx = cx - node.x;
        const dy = cy - node.y;
        node.vx += dx * 0.005;
        node.vy += dy * 0.005;
      });

      nodes.forEach(node => {
        if (node === draggedNode) return;
        node.x += node.vx;
        node.y += node.vy;
        node.vx *= 0.85;
        node.vy *= 0.85;
        
        node.x = Math.max(20, Math.min(canvas.width - 20, node.x));
        node.y = Math.max(20, Math.min(canvas.height - 20, node.y));
      });

      ctx.clearRect(0, 0, canvas.width, canvas.height);

      ctx.strokeStyle = '#18181b';
      ctx.lineWidth = 1;
      const gridSize = 40;
      for (let x = 0; x < canvas.width; x += gridSize) {
        ctx.beginPath();
        ctx.moveTo(x, 0);
        ctx.lineTo(x, canvas.height);
        ctx.stroke();
      }
      for (let y = 0; y < canvas.height; y += gridSize) {
        ctx.beginPath();
        ctx.moveTo(0, y);
        ctx.lineTo(canvas.width, y);
        ctx.stroke();
      }

      ctx.strokeStyle = '#3f3f46';
      ctx.lineWidth = 1.5;
      edges.forEach(edge => {
        ctx.beginPath();
        ctx.moveTo(edge.source.x, edge.source.y);
        ctx.lineTo(edge.target.x, edge.target.y);
        ctx.stroke();
      });

      nodes.forEach(node => {
        const isCurrent = node.id === selectedNoteName.toLowerCase();
        
        ctx.beginPath();
        ctx.arc(node.x, node.y, node.radius + (isCurrent ? 6 : 4), 0, Math.PI * 2);
        ctx.fillStyle = isCurrent ? 'rgba(99, 102, 241, 0.15)' : 'rgba(39, 39, 42, 0.4)';
        ctx.fill();

        ctx.beginPath();
        ctx.arc(node.x, node.y, node.radius, 0, Math.PI * 2);
        
        if (isCurrent) {
          ctx.fillStyle = '#818cf8';
        } else if (node.isGlobal) {
          ctx.fillStyle = '#f97316';
        } else {
          ctx.fillStyle = '#6366f1';
        }
        ctx.fill();

        ctx.font = isCurrent ? 'bold 12px sans-serif' : '11px sans-serif';
        ctx.fillStyle = isCurrent ? '#f4f4f5' : '#a1a1aa';
        ctx.textAlign = 'center';
        ctx.fillText(node.name, node.x, node.y - node.radius - 8);
      });

      animationId = requestAnimationFrame(step);
    };

    const getMousePos = (e) => {
      const rect = canvas.getBoundingClientRect();
      return {
        x: e.clientX - rect.left,
        y: e.clientY - rect.top,
      };
    };

    const handleMouseDown = (e) => {
      const pos = getMousePos(e);
      const clicked = nodes.find(node => {
        const dx = node.x - pos.x;
        const dy = node.y - pos.y;
        return Math.sqrt(dx * dx + dy * dy) < node.radius + 10;
      });

      if (clicked) {
        draggedNode = clicked;
        setSelectedNoteName(clicked.id);
      }
    };

    const handleMouseMove = (e) => {
      if (!draggedNode) return;
      const pos = getMousePos(e);
      draggedNode.x = pos.x;
      draggedNode.y = pos.y;
    };

    const handleMouseUp = () => {
      draggedNode = null;
    };

    canvas.addEventListener('mousedown', handleMouseDown);
    canvas.addEventListener('mousemove', handleMouseMove);
    window.addEventListener('mouseup', handleMouseUp);

    animationId = requestAnimationFrame(step);

    return () => {
      cancelAnimationFrame(animationId);
      window.removeEventListener('resize', resizeCanvas);
      canvas.removeEventListener('mousedown', handleMouseDown);
      canvas.removeEventListener('mousemove', handleMouseMove);
      window.removeEventListener('mouseup', handleMouseUp);
    };
  }, [viewMode, memories, selectedNoteName]);

  const totalNotes = memories.length;
  const globalNotesCount = memories.filter(m => m.file_path.includes('.config')).length;
  const outdatedNotesCount = memories.filter(m => (m.frontmatter.references || []).some(r => r.status && r.status !== 'OK')).length;

  return (
    <div className="flex h-screen bg-zinc-950 text-zinc-100 overflow-hidden font-sans">
      <div className="w-80 border-r border-zinc-900 bg-zinc-900/20 backdrop-blur-xl flex flex-col h-full select-none">
        <div className="p-5 border-b border-zinc-900 flex items-center space-x-3">
          <div className="p-2 bg-indigo-600/10 border border-indigo-500/20 rounded-xl text-indigo-400">
            <Compass size={22} className="animate-pulse" />
          </div>
          <div>
            <h1 className="text-md font-bold tracking-tight bg-gradient-to-r from-indigo-200 to-indigo-400 bg-clip-text text-transparent">
              Brainwares Vault
            </h1>
            <p className="text-xs text-zinc-500 font-mono">CLI UI v0.1.0</p>
          </div>
        </div>

        <div className="p-4 bg-zinc-900/40 border-b border-zinc-900 grid grid-cols-3 gap-2 text-center">
          <div className="p-2 bg-zinc-950/40 rounded-lg border border-zinc-900">
            <div className="text-xs text-zinc-500">Total</div>
            <div className="text-lg font-bold font-mono text-zinc-200">{totalNotes}</div>
          </div>
          <div className="p-2 bg-zinc-950/40 rounded-lg border border-zinc-900">
            <div className="text-xs text-zinc-500">Global</div>
            <div className="text-lg font-bold font-mono text-orange-500">{globalNotesCount}</div>
          </div>
          <div className="p-2 bg-zinc-950/40 rounded-lg border border-zinc-900">
            <div className="text-xs text-zinc-500">Outdated</div>
            <div className="text-lg font-bold font-mono text-red-400">{outdatedNotesCount}</div>
          </div>
        </div>

        <div className="p-4 space-y-3">
          <div className="relative">
            <Search className="absolute left-3 top-2.5 text-zinc-500" size={16} />
            <input
              type="text"
              placeholder="Search memories..."
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              className="w-full bg-zinc-950 border border-zinc-900 rounded-lg pl-9 pr-4 py-2 text-sm text-zinc-200 placeholder-zinc-600 focus:outline-none focus:border-indigo-500/50 transition-colors"
            />
          </div>

          <div className="flex flex-wrap gap-1 items-center py-1">
            <button
              onClick={() => setSelectedTag(null)}
              className={`px-2 py-1 rounded text-xs transition-colors flex items-center space-x-1 ${!selectedTag ? 'bg-indigo-600/20 text-indigo-400 border border-indigo-500/20' : 'bg-zinc-950 text-zinc-500 hover:text-zinc-300'}`}
            >
              <span>All</span>
            </button>
            {allTags.map(tag => (
              <button
                key={tag}
                onClick={() => setSelectedTag(selectedTag === tag ? null : tag)}
                className={`px-2 py-1 rounded text-xs transition-colors flex items-center space-x-1 ${selectedTag === tag ? 'bg-indigo-600/20 text-indigo-400 border border-indigo-500/20' : 'bg-zinc-950 text-zinc-500 hover:text-zinc-300'}`}
              >
                <Tag size={10} />
                <span>{tag}</span>
              </button>
            ))}
          </div>
        </div>

        <div className="flex-1 overflow-y-auto px-4 pb-4 space-y-1">
          {filteredNotes.map(m => {
            const isCurrent = m.name.toLowerCase() === selectedNoteName.toLowerCase();
            const isGlobal = m.file_path.includes('.config');
            const hasOutdated = (m.frontmatter.references || []).some(r => r.status && r.status !== 'OK');

            return (
              <button
                key={m.name}
                onClick={() => setSelectedNoteName(m.name)}
                className={`w-full text-left p-3 rounded-xl transition-all duration-200 flex flex-col space-y-1 border ${isCurrent ? 'bg-indigo-600/10 border-indigo-500/40 text-indigo-200 shadow-lg shadow-indigo-500/5' : 'bg-transparent border-transparent hover:bg-zinc-900/40 hover:border-zinc-900 text-zinc-400 hover:text-zinc-200'}`}
              >
                <div className="flex justify-between items-start w-full">
                  <span className="font-medium text-sm truncate">{m.frontmatter.title || m.name}</span>
                  <div className="flex space-x-1 items-center flex-shrink-0">
                    {isGlobal && (
                      <span className="px-1.5 py-0.5 rounded text-[9px] bg-orange-950 border border-orange-500/20 text-orange-400 font-semibold font-mono">G</span>
                    )}
                    {hasOutdated && (
                      <AlertCircle size={12} className="text-red-400" />
                    )}
                  </div>
                </div>
                <div className="flex justify-between items-center w-full text-[10px] text-zinc-600 font-mono">
                  <span>[[{m.name}]]</span>
                  {m.frontmatter.tags && m.frontmatter.tags.length > 0 && (
                    <span className="truncate max-w-[120px]">#{m.frontmatter.tags[0]}</span>
                  )}
                </div>
              </button>
            );
          })}

          {filteredNotes.length === 0 && (
            <div className="p-8 text-center text-xs text-zinc-600">
              No matching memory notes.
            </div>
          )}
        </div>
      </div>

      <div className="flex-1 flex flex-col h-full bg-zinc-950 overflow-hidden relative">
        <div className="h-16 border-b border-zinc-900 px-6 flex justify-between items-center bg-zinc-900/10 backdrop-blur-md z-10 select-none">
          <div className="flex items-center space-x-4">
            <button
              onClick={() => setViewMode('doc')}
              className={`flex items-center space-x-2 px-3 py-1.5 rounded-lg text-sm transition-colors border ${viewMode === 'doc' ? 'bg-zinc-900 border-zinc-800 text-zinc-100' : 'bg-transparent border-transparent text-zinc-500 hover:text-zinc-300'}`}
            >
              <BookOpen size={16} />
              <span>Document</span>
            </button>
            <button
              onClick={() => setViewMode('graph')}
              className={`flex items-center space-x-2 px-3 py-1.5 rounded-lg text-sm transition-colors border ${viewMode === 'graph' ? 'bg-zinc-900 border-zinc-800 text-zinc-100' : 'bg-transparent border-transparent text-zinc-500 hover:text-zinc-300'}`}
            >
              <Network size={16} />
              <span>Visualizer</span>
            </button>
          </div>

          <div className="text-xs text-zinc-500 font-mono flex items-center space-x-2">
            <span>Workspace:</span>
            <span className="text-zinc-300 bg-zinc-900 px-2 py-1 rounded border border-zinc-800 truncate max-w-xs">
              {data.vault_path}
            </span>
          </div>
        </div>

        <div className="flex-1 overflow-hidden relative">
          {viewMode === 'doc' ? (
            <div className="flex h-full overflow-hidden">
              <div className="flex-1 overflow-y-auto px-10 py-8">
                {selectedNote ? (
                  <article className="max-w-3xl mx-auto prose prose-invert prose-indigo">
                    <div className="mb-8 border-b border-zinc-900 pb-6">
                      <div className="flex flex-wrap gap-2 mb-3">
                        {(selectedNote.frontmatter.tags || []).map(t => (
                          <span key={t} className="px-2 py-0.5 rounded-full text-xs bg-zinc-900 border border-zinc-800 text-zinc-400 flex items-center space-x-1">
                            <Tag size={10} />
                            <span>{t}</span>
                          </span>
                        ))}
                        {selectedNote.file_path.includes('.config') && (
                          <span className="px-2 py-0.5 rounded-full text-xs bg-orange-950 border border-orange-500/20 text-orange-400 font-semibold font-mono">
                            Global User Preference
                          </span>
                        )}
                      </div>

                      <h1 className="text-3xl font-bold tracking-tight text-zinc-100 mb-2">
                        {selectedNote.frontmatter.title || selectedNote.name}
                      </h1>
                      
                      <div className="text-xs text-zinc-500 font-mono">
                        Last Updated: {selectedNote.frontmatter.last_updated || 'Unknown'}
                      </div>
                    </div>

                    <div 
                      onClick={handleHtmlClick}
                      className="markdown-body text-zinc-300 leading-relaxed space-y-4"
                      dangerouslySetInnerHTML={{ __html: marked.parse(preprocessMarkdown(selectedNote.body)) }}
                    />
                  </article>
                ) : (
                  <div className="flex items-center justify-center h-full text-zinc-500">
                    No note selected. Select a note from the sidebar.
                  </div>
                )}
              </div>

              <div className="w-80 border-l border-zinc-900 bg-zinc-900/10 flex flex-col overflow-y-auto p-6 space-y-6">
                {selectedNote && (
                  <>
                    <div className="space-y-3">
                      <h3 className="text-xs font-bold uppercase tracking-wider text-zinc-500 flex items-center space-x-2">
                        <FileCode size={14} />
                        <span>Code References</span>
                      </h3>
                      
                      <div className="space-y-2">
                        {selectedNote.frontmatter.references && selectedNote.frontmatter.references.length > 0 ? (
                          selectedNote.frontmatter.references.map(ref => {
                            const isOk = ref.status === 'OK';
                            return (
                              <div key={ref.file_path} className="p-3 bg-zinc-900/40 border border-zinc-900 rounded-xl flex items-center justify-between">
                                <div className="min-w-0 flex-1 pr-2">
                                  <div className="text-xs font-mono truncate text-zinc-300" title={ref.file_path}>
                                    {ref.file_path.split('/').pop()}
                                  </div>
                                  <div className="text-[10px] text-zinc-600 truncate">{ref.file_path}</div>
                                </div>
                                <div className="flex-shrink-0">
                                  {isOk ? (
                                    <span className="px-2 py-0.5 rounded-full text-[10px] bg-emerald-950 border border-emerald-500/20 text-emerald-400 font-medium flex items-center space-x-1">
                                      <CheckCircle size={10} />
                                      <span>OK</span>
                                    </span>
                                  ) : (
                                    <span className="px-2 py-0.5 rounded-full text-[10px] bg-red-950 border border-red-500/20 text-red-400 font-medium flex items-center space-x-1">
                                      <AlertCircle size={10} />
                                      <span>Outdated</span>
                                    </span>
                                  )}
                                </div>
                              </div>
                            );
                          })
                        ) : (
                          <div className="text-xs text-zinc-600 italic">No code references linked to this note.</div>
                        )}
                      </div>
                    </div>

                    <div className="space-y-3">
                      <h3 className="text-xs font-bold uppercase tracking-wider text-zinc-500 flex items-center space-x-2">
                        <Link2 size={14} />
                        <span>Backlinks</span>
                      </h3>

                      <div className="space-y-2">
                        {selectedNote.backlinks && selectedNote.backlinks.length > 0 ? (
                          selectedNote.backlinks.map(bl => (
                            <button
                              key={bl.source}
                              onClick={() => setSelectedNoteName(bl.source)}
                              className="w-full text-left p-3 bg-zinc-900/40 hover:bg-zinc-900/70 border border-zinc-900 hover:border-zinc-800 rounded-xl transition-all duration-200 flex flex-col space-y-1"
                            >
                              <div className="text-xs font-semibold text-zinc-300">
                                {bl.source}
                              </div>
                              <div className="text-[10px] text-zinc-500 italic truncate">
                                "{bl.context_line}"
                              </div>
                            </button>
                          ))
                        ) : (
                          <div className="text-xs text-zinc-600 italic">No incoming links to this note.</div>
                        )}
                      </div>
                    </div>
                  </>
                )}
              </div>
            </div>
          ) : (
            <div className="w-full h-full relative overflow-hidden bg-zinc-950">
              <canvas ref={canvasRef} className="block w-full h-full cursor-grab active:cursor-grabbing" />
              
              <div className="absolute bottom-6 left-6 p-4 bg-zinc-900/80 backdrop-blur-md border border-zinc-800 rounded-xl text-xs space-y-2 select-none text-zinc-300">
                <h4 className="font-bold text-zinc-200 mb-1">Legend</h4>
                <div className="flex items-center space-x-2">
                  <span className="w-3 h-3 rounded-full bg-indigo-400" />
                  <span>Current Node</span>
                </div>
                <div className="flex items-center space-x-2">
                  <span className="w-3 h-3 rounded-full bg-indigo-500" />
                  <span>Local Memory</span>
                </div>
                <div className="flex items-center space-x-2">
                  <span className="w-3 h-3 rounded-full bg-orange-500" />
                  <span>Global Preference</span>
                </div>
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}