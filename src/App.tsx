import React, { useEffect, useState, useRef } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'

interface LogEntry {
  id: number;
  timestamp: string;
  message: string;
  type: 'info' | 'error' | 'success';
}

interface PortInfo {
  path: string;
  manufacturer: string;
}

interface LogPayload {
  message: string;
  type: 'info' | 'error' | 'success';
}

function App() {
  const [ports, setPorts] = useState<PortInfo[]>([])
  const [selectedPort, setSelectedPort] = useState<string>('')
  const [isConnected, setIsConnected] = useState(false)
  const [font, setFont] = useState<number>(0)
  const [logs, setLogs] = useState<LogEntry[]>([])
  const logCounter = useRef(0)
  const scrollRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    // Get initial ports
    invoke<PortInfo[]>('get_ports').then(p => {
      setPorts(p)
      if (p.length > 0) setSelectedPort(p[0].path)
    }).catch(err => addLog(`Failed to list ports: ${err}`, 'error'))

    // Listen to logs
    let unlistenFn: () => void;
    listen<LogPayload>('printer-log', (event) => {
      addLog(event.payload.message, event.payload.type)
    }).then(unlisten => {
      unlistenFn = unlisten;
    });

    return () => {
      if (unlistenFn) unlistenFn()
    }
  }, [])

  useEffect(() => {
    if (scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight
    }
  }, [logs])

  const addLog = (message: string, type: 'info' | 'error' | 'success' = 'info') => {
    logCounter.current += 1
    const ts = new Date().toISOString().split('T')[1].slice(0, 12)
    setLogs(prev => [...prev, { id: logCounter.current, timestamp: ts, message, type }])
  }

  const handleConnect = async () => {
    if (isConnected) {
      const success = await invoke<boolean>('disconnect_port')
      if (success) setIsConnected(false)
    } else {
      if (!selectedPort) return
      const success = await invoke<boolean>('connect_port', { portPath: selectedPort })
      setIsConnected(success)
    }
  }

  const sendCommand = async (bytes: number[], desc: string) => {
    if (!isConnected) {
      addLog(`Cannot send ${desc} - Not connected`, 'error')
      return
    }
    await invoke<boolean>('send_command', { bytes })
  }

  const handleFontChange = async (e: React.ChangeEvent<HTMLInputElement>) => {
    const val = parseInt(e.target.value)
    setFont(val)
    await sendCommand([0x1B, 0x78, val], `Font: ${val === 0 ? 'Draft' : val === 1 ? 'Roman' : 'Sans Serif'}`)
  }

  const handleTestPrint = async () => {
    const dateStr = new Date().toLocaleString()
    const text = `================================\nTEST PRINT - LX350 Panel\n--------------------------------\nDate: ${dateStr}\nItem 1........................5.00\nItem 2.......................10.00\n--------------------------------\nTOTAL........................15.00\n================================\n\n\n\n\n\n`
    
    const bytes = Array.from(text).map(c => c.charCodeAt(0))
    await sendCommand(bytes, 'Test Print Block')
  }

  return (
    <div className="h-screen flex flex-col bg-background text-foreground p-4 gap-4">
      {/* Header / Port Selection */}
      <div className="flex items-center gap-4 bg-card p-4 rounded-md border border-border shadow-sm">
        <h1 className="text-xl font-bold uppercase tracking-wider text-primary">LX350 Panel</h1>
        
        <div className="flex-1 flex items-center gap-3 justify-end">
          <div className="flex items-center gap-2">
            <div className={`w-3 h-3 rounded-full ${isConnected ? 'bg-terminal-green shadow-[0_0_8px_#00ff00]' : 'bg-red-500 shadow-[0_0_8px_#ef4444]'}`}></div>
            <span className="text-sm font-bold uppercase">{isConnected ? 'Connected' : 'Offline'}</span>
          </div>
          
          <select 
            className="bg-input border border-border rounded px-3 py-2 text-sm outline-none focus:ring-2 focus:ring-ring"
            value={selectedPort}
            onChange={(e) => setSelectedPort(e.target.value)}
            disabled={isConnected}
          >
            <option value="" disabled>Select Port...</option>
            {ports.map(p => (
              <option key={p.path} value={p.path}>{p.path} {p.manufacturer ? `(${p.manufacturer})` : ''}</option>
            ))}
          </select>
          
          <button 
            onClick={handleConnect}
            className={`physical-btn ${isConnected ? '!bg-red-900 !border-red-700 !text-red-100 hover:!bg-red-800' : ''}`}
          >
            {isConnected ? 'DISCONNECT' : 'CONNECT'}
          </button>
        </div>
      </div>

      <div className="flex flex-1 gap-4 overflow-hidden">
        {/* Left Panel: Controls */}
        <div className="w-1/2 flex flex-col gap-4 overflow-y-auto">
          
          {/* Commands */}
          <div className="bg-card p-4 rounded-md border border-border flex flex-col gap-3">
            <h2 className="text-sm font-bold text-muted-foreground uppercase border-b border-border pb-2">Printer Commands</h2>
            <div className="grid grid-cols-2 gap-3">
              <button className="physical-btn" onClick={() => sendCommand([0x0A], 'LF')}>LF (Line Feed)</button>
              <button className="physical-btn" onClick={() => sendCommand([0x0C], 'FF')}>FF (Form Feed)</button>
              <button className="physical-btn" onClick={() => sendCommand([0x1B, 0x43, 0x0C], 'Tear Off')}>Tear Off</button>
              <button className="physical-btn" onClick={() => sendCommand([0x1B, 0x40], 'Initialize')}>Initialize</button>
            </div>
          </div>

          {/* Micro Adjust */}
          <div className="bg-card p-4 rounded-md border border-border flex flex-col gap-3">
            <h2 className="text-sm font-bold text-muted-foreground uppercase border-b border-border pb-2">Micro Adjust</h2>
            <div className="flex gap-3">
              <button className="physical-btn flex-1 text-xl" onClick={() => sendCommand([0x1B, 0x2B], 'Micro Up')}>▲ UP (ESC +)</button>
              <button className="physical-btn flex-1 text-xl" onClick={() => sendCommand([0x1B, 0x2D], 'Micro Down')}>▼ DOWN (ESC -)</button>
            </div>
          </div>

          {/* Fonts */}
          <div className="bg-card p-4 rounded-md border border-border flex flex-col gap-3">
            <h2 className="text-sm font-bold text-muted-foreground uppercase border-b border-border pb-2">Font Selector</h2>
            <div className="flex flex-col gap-2">
              <label className="flex items-center gap-3 cursor-pointer p-2 hover:bg-secondary rounded border border-transparent hover:border-border transition-colors">
                <input type="radio" name="font" value={0} checked={font === 0} onChange={handleFontChange} className="w-4 h-4 text-primary bg-input border-border focus:ring-primary focus:ring-2" />
                <span>Draft (ESC x 0)</span>
              </label>
              <label className="flex items-center gap-3 cursor-pointer p-2 hover:bg-secondary rounded border border-transparent hover:border-border transition-colors">
                <input type="radio" name="font" value={1} checked={font === 1} onChange={handleFontChange} className="w-4 h-4 text-primary bg-input border-border focus:ring-primary focus:ring-2" />
                <span>Roman (ESC x 1)</span>
              </label>
              <label className="flex items-center gap-3 cursor-pointer p-2 hover:bg-secondary rounded border border-transparent hover:border-border transition-colors">
                <input type="radio" name="font" value={2} checked={font === 2} onChange={handleFontChange} className="w-4 h-4 text-primary bg-input border-border focus:ring-primary focus:ring-2" />
                <span>Sans Serif (ESC x 2)</span>
              </label>
            </div>
          </div>

          {/* Test Print */}
          <div className="bg-card p-4 rounded-md border border-border flex flex-col gap-3 mt-auto">
            <button className="physical-btn bg-primary text-primary-foreground border-t-green-500 border-l-green-500 border-b-green-900 border-r-green-900 hover:bg-green-600 active:border-t-green-900 active:border-l-green-900 active:border-b-green-500 active:border-r-green-500" onClick={handleTestPrint}>
              PRINT TEST RECEIPT
            </button>
          </div>
          
        </div>

        {/* Right Panel: Command Log Terminal */}
        <div className="w-1/2 flex flex-col bg-[#0a0a0a] border-2 border-border rounded-md overflow-hidden relative shadow-[inset_0_0_20px_rgba(0,0,0,0.8)]">
          <div className="bg-industrial-800 text-xs font-bold px-3 py-1 text-muted-foreground border-b border-border flex justify-between">
            <span>COMMAND_LOG.EXE</span>
            <span className="text-terminal-green-dim">READY</span>
          </div>
          <div 
            ref={scrollRef}
            className="flex-1 overflow-y-auto p-4 font-mono text-sm leading-relaxed"
          >
            {logs.map(log => (
              <div key={log.id} className="flex gap-3 mb-1">
                <span className="text-industrial-600 select-none">[{log.timestamp}]</span>
                <span className={`flex-1 break-all ${log.type === 'error' ? 'text-red-500' : log.type === 'success' ? 'text-terminal-green' : 'text-terminal-green-dim'}`}>
                  {log.message}
                </span>
              </div>
            ))}
            {logs.length === 0 && (
              <div className="text-industrial-600 italic">Waiting for commands...</div>
            )}
          </div>
          {/* CRT Scanline effect */}
          <div className="absolute inset-0 pointer-events-none bg-[linear-gradient(rgba(18,16,16,0)_50%,rgba(0,0,0,0.25)_50%),linear-gradient(90deg,rgba(255,0,0,0.06),rgba(0,255,0,0.02),rgba(0,0,255,0.06))] bg-[length:100%_4px,3px_100%] opacity-20"></div>
        </div>
      </div>
    </div>
  )
}

export default App
