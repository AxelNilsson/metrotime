# MetroTimes3 Architecture

## Overview

MetroTimes3 uses a sophisticated multi-core, lock-free architecture to achieve butter-smooth rendering with zero frame drops. This document explains the key architectural decisions and how they work together.

## System Architecture

```svg
<svg viewBox="0 0 800 600" xmlns="http://www.w3.org/2000/svg">
  <!-- Title -->
  <text x="400" y="30" text-anchor="middle" font-size="20" font-weight="bold" fill="#333">
    MetroTimes3 Dual-Core Architecture
  </text>

  <!-- Core 0 Box -->
  <rect x="50" y="60" width="300" height="480" fill="#e3f2fd" stroke="#1976d2" stroke-width="3" rx="10"/>
  <text x="200" y="90" text-anchor="middle" font-size="18" font-weight="bold" fill="#1976d2">
    Core 0 (Network/API)
  </text>

  <!-- Core 0 Tasks -->
  <rect x="70" y="110" width="260" height="80" fill="#fff" stroke="#1976d2" stroke-width="2" rx="5"/>
  <text x="200" y="135" text-anchor="middle" font-size="14" font-weight="bold">Data Fetch Task</text>
  <text x="200" y="155" text-anchor="middle" font-size="12" fill="#666">WiFi/HTTP/TLS</text>
  <text x="200" y="175" text-anchor="middle" font-size="12" fill="#666">JSON Parsing (256KB)</text>

  <rect x="70" y="210" width="260" height="60" fill="#fff" stroke="#1976d2" stroke-width="2" rx="5"/>
  <text x="200" y="235" text-anchor="middle" font-size="14" font-weight="bold">Network Stack</text>
  <text x="200" y="255" text-anchor="middle" font-size="12" fill="#666">Embassy-net/DHCP/DNS</text>

  <!-- PSRAM box -->
  <rect x="70" y="290" width="260" height="80" fill="#fff9c4" stroke="#f57f17" stroke-width="2" rx="5"/>
  <text x="200" y="315" text-anchor="middle" font-size="14" font-weight="bold">PSRAM (2MB)</text>
  <text x="200" y="335" text-anchor="middle" font-size="12" fill="#666">API Buffer: 256KB</text>
  <text x="200" y="355" text-anchor="middle" font-size="12" fill="#666">Heap Allocations</text>

  <!-- Channel -->
  <rect x="70" y="390" width="260" height="60" fill="#c8e6c9" stroke="#388e3c" stroke-width="2" rx="5"/>
  <text x="200" y="415" text-anchor="middle" font-size="14" font-weight="bold">Lock-Free Channel</text>
  <text x="200" y="435" text-anchor="middle" font-size="12" fill="#666">.send() to Core 1</text>

  <!-- Internal SRAM -->
  <rect x="70" y="470" width="260" height="60" fill="#fff" stroke="#666" stroke-width="2" rx="5"/>
  <text x="200" y="495" text-anchor="middle" font-size="14" font-weight="bold">Internal SRAM</text>
  <text x="200" y="515" text-anchor="middle" font-size="12" fill="#666">120KB for WiFi/System</text>

  <!-- Core 1 Box -->
  <rect x="450" y="60" width="300" height="480" fill="#f3e5f5" stroke="#7b1fa2" stroke-width="3" rx="10"/>
  <text x="600" y="90" text-anchor="middle" font-size="18" font-weight="bold" fill="#7b1fa2">
    Core 1 (Display Only)
  </text>

  <!-- Core 1 Tasks -->
  <rect x="470" y="110" width="260" height="80" fill="#fff" stroke="#7b1fa2" stroke-width="2" rx="5"/>
  <text x="600" y="135" text-anchor="middle" font-size="14" font-weight="bold">Display Refresh</text>
  <text x="600" y="155" text-anchor="middle" font-size="12" fill="#666">High Priority (200Hz)</text>
  <text x="600" y="175" text-anchor="middle" font-size="12" fill="#666">Reads fb0 OR fb1</text>

  <rect x="470" y="210" width="260" height="80" fill="#fff" stroke="#7b1fa2" stroke-width="2" rx="5"/>
  <text x="600" y="235" text-anchor="middle" font-size="14" font-weight="bold">Render+Scroll Task</text>
  <text x="600" y="255" text-anchor="middle" font-size="12" fill="#666">Low Priority (100Hz)</text>
  <text x="600" y="275" text-anchor="middle" font-size="12" fill="#666">Writes fb1 OR fb0</text>

  <!-- Double Buffering -->
  <rect x="470" y="310" width="120" height="70" fill="#ffccbc" stroke="#d84315" stroke-width="2" rx="5"/>
  <text x="530" y="335" text-anchor="middle" font-size="14" font-weight="bold">FB0</text>
  <text x="530" y="355" text-anchor="middle" font-size="11" fill="#666">Mutex 0</text>
  <text x="530" y="370" text-anchor="middle" font-size="11" fill="#666">~200KB</text>

  <rect x="610" y="310" width="120" height="70" fill="#ffccbc" stroke="#d84315" stroke-width="2" rx="5"/>
  <text x="670" y="335" text-anchor="middle" font-size="14" font-weight="bold">FB1</text>
  <text x="670" y="355" text-anchor="middle" font-size="11" fill="#666">Mutex 1</text>
  <text x="670" y="370" text-anchor="middle" font-size="11" fill="#666">~200KB</text>

  <!-- Atomic Swap -->
  <path d="M 530 390 Q 600 410 670 390" fill="none" stroke="#d84315" stroke-width="2" marker-end="url(#arrowhead)"/>
  <path d="M 670 395 Q 600 415 530 395" fill="none" stroke="#d84315" stroke-width="2" marker-end="url(#arrowhead)"/>
  <text x="600" y="430" text-anchor="middle" font-size="12" fill="#d84315" font-weight="bold">
    Atomic Swap
  </text>

  <!-- HUB75 Display -->
  <rect x="470" y="460" width="260" height="70" fill="#fff" stroke="#666" stroke-width="2" rx="5"/>
  <text x="600" y="485" text-anchor="middle" font-size="14" font-weight="bold">HUB75 Display</text>
  <text x="600" y="505" text-anchor="middle" font-size="12" fill="#666">32×128 (2 screens)</text>
  <text x="600" y="520" text-anchor="middle" font-size="12" fill="#666">DMA Refresh</text>

  <!-- Arrow from Core 0 to Core 1 -->
  <defs>
    <marker id="arrowhead" markerWidth="10" markerHeight="10" refX="9" refY="3" orient="auto">
      <polygon points="0 0, 10 3, 0 6" fill="#388e3c" />
    </marker>
  </defs>
  <path d="M 330 420 L 450 420" fill="none" stroke="#388e3c" stroke-width="3" marker-end="url(#arrowhead)"/>
  <text x="390" y="410" text-anchor="middle" font-size="12" fill="#388e3c" font-weight="bold">
    Channel (Lock-Free!)
  </text>
</svg>
```

## Three-Layer Stutter Elimination

### Layer 1: Core Isolation

**Problem:** Network operations (WiFi, HTTP parsing) are unpredictable and can cause delays.

**Solution:** Separate cores for separate concerns:
- **Core 0:** Handles all network/API operations (can stutter without affecting display)
- **Core 1:** Dedicated to display refresh and rendering (consistent, smooth)

```svg
<svg viewBox="0 0 600 300" xmlns="http://www.w3.org/2000/svg">
  <text x="300" y="30" text-anchor="middle" font-size="18" font-weight="bold">
    Core Isolation
  </text>

  <!-- Timeline -->
  <line x1="50" y1="100" x2="550" y2="100" stroke="#333" stroke-width="2"/>
  <text x="50" y="90" font-size="12">0ms</text>
  <text x="550" y="90" font-size="12">100ms</text>

  <!-- Core 0 operations -->
  <rect x="50" y="120" width="100" height="40" fill="#e3f2fd" stroke="#1976d2" stroke-width="2"/>
  <text x="100" y="145" text-anchor="middle" font-size="11">HTTP</text>

  <rect x="200" y="120" width="150" height="40" fill="#ffccbc" stroke="#d84315" stroke-width="2"/>
  <text x="275" y="145" text-anchor="middle" font-size="11">JSON Parse (slow)</text>

  <rect x="380" y="120" width="60" height="40" fill="#c8e6c9" stroke="#388e3c" stroke-width="2"/>
  <text x="410" y="145" text-anchor="middle" font-size="11">Send</text>

  <text x="50" y="110" font-size="12" font-weight="bold" fill="#1976d2">Core 0:</text>

  <!-- Core 1 operations (consistent) -->
  <rect x="50" y="180" width="20" height="40" fill="#f3e5f5" stroke="#7b1fa2" stroke-width="2"/>
  <rect x="75" y="180" width="20" height="40" fill="#f3e5f5" stroke="#7b1fa2" stroke-width="2"/>
  <rect x="100" y="180" width="20" height="40" fill="#f3e5f5" stroke="#7b1fa2" stroke-width="2"/>
  <rect x="125" y="180" width="20" height="40" fill="#f3e5f5" stroke="#7b1fa2" stroke-width="2"/>
  <rect x="150" y="180" width="20" height="40" fill="#f3e5f5" stroke="#7b1fa2" stroke-width="2"/>
  <text x="100" y="205" text-anchor="middle" font-size="10">...</text>

  <text x="50" y="170" font-size="12" font-weight="bold" fill="#7b1fa2">Core 1:</text>
  <text x="300" y="205" text-anchor="middle" font-size="11" fill="#7b1fa2">
    Smooth 200Hz refresh (unaffected by Core 0!)
  </text>
</svg>
```

### Layer 2: Lock-Free Channel

**Problem:** Sharing data via `Mutex` causes contention - Core 1 blocks waiting for Core 0.

**Solution:** One-way `Channel` from Core 0 → Core 1:
- Core 0: `.send()` when new data ready (non-blocking)
- Core 1: `.try_receive()` to check for updates (non-blocking)
- No mutex = no waiting = no stutters!

```svg
<svg viewBox="0 0 600 350" xmlns="http://www.w3.org/2000/svg">
  <text x="300" y="30" text-anchor="middle" font-size="18" font-weight="bold">
    Lock-Free Channel vs Mutex
  </text>

  <!-- OLD WAY: Mutex -->
  <text x="150" y="70" text-anchor="middle" font-size="14" font-weight="bold" fill="#d32f2f">
    ❌ OLD: Shared Mutex
  </text>

  <rect x="50" y="90" width="200" height="100" fill="#ffebee" stroke="#d32f2f" stroke-width="2" rx="5"/>
  <text x="150" y="115" text-anchor="middle" font-size="12" font-weight="bold">Mutex&lt;DepartureData&gt;</text>

  <circle cx="90" cy="150" r="15" fill="#e3f2fd" stroke="#1976d2" stroke-width="2"/>
  <text x="90" y="155" text-anchor="middle" font-size="11" font-weight="bold">C0</text>
  <line x1="105" y1="150" x2="130" y2="150" stroke="#d32f2f" stroke-width="2"/>
  <text x="135" y="155" font-size="10" fill="#d32f2f">lock()</text>

  <circle cx="210" cy="150" r="15" fill="#f3e5f5" stroke="#7b1fa2" stroke-width="2"/>
  <text x="210" y="155" text-anchor="middle" font-size="11" font-weight="bold">C1</text>
  <line x1="195" y1="150" x2="170" y2="150" stroke="#d32f2f" stroke-width="2"/>
  <text x="165" y="145" font-size="10" fill="#d32f2f" text-anchor="end">BLOCKED!</text>

  <!-- NEW WAY: Channel -->
  <text x="450" y="70" text-anchor="middle" font-size="14" font-weight="bold" fill="#388e3c">
    ✅ NEW: Lock-Free Channel
  </text>

  <rect x="350" y="90" width="200" height="100" fill="#e8f5e9" stroke="#388e3c" stroke-width="2" rx="5"/>
  <text x="450" y="115" text-anchor="middle" font-size="12" font-weight="bold">Channel&lt;DepartureData, 1&gt;</text>

  <circle cx="390" cy="150" r="15" fill="#e3f2fd" stroke="#1976d2" stroke-width="2"/>
  <text x="390" y="155" text-anchor="middle" font-size="11" font-weight="bold">C0</text>
  <path d="M 405 150 L 430 150" fill="none" stroke="#388e3c" stroke-width="2" marker-end="url(#arrow-green)"/>
  <text x="435" y="145" font-size="10" fill="#388e3c">send()</text>

  <circle cx="510" cy="150" r="15" fill="#f3e5f5" stroke="#7b1fa2" stroke-width="2"/>
  <text x="510" y="155" text-anchor="middle" font-size="11" font-weight="bold">C1</text>
  <path d="M 495 150 L 470 150" fill="none" stroke="#388e3c" stroke-width="2" marker-end="url(#arrow-green)"/>
  <text x="465" y="165" font-size="10" fill="#388e3c" text-anchor="end">try_recv()</text>

  <text x="450" y="180" text-anchor="middle" font-size="11" fill="#388e3c" font-style="italic">
    No blocking!
  </text>

  <defs>
    <marker id="arrow-green" markerWidth="8" markerHeight="8" refX="7" refY="3" orient="auto">
      <polygon points="0 0, 8 3, 0 6" fill="#388e3c" />
    </marker>
  </defs>

  <!-- Comparison table -->
  <rect x="50" y="220" width="500" height="100" fill="#fff" stroke="#666" stroke-width="1" rx="5"/>
  <line x1="50" y1="250" x2="550" y2="250" stroke="#666" stroke-width="1"/>
  <line x1="300" y1="220" x2="300" y2="320" stroke="#666" stroke-width="1"/>

  <text x="175" y="240" text-anchor="middle" font-size="12" font-weight="bold">Mutex (Old)</text>
  <text x="425" y="240" text-anchor="middle" font-size="12" font-weight="bold">Channel (New)</text>

  <text x="60" y="270" font-size="11">• Core 1 blocks waiting</text>
  <text x="60" y="290" font-size="11">• Microstutters every 20s</text>
  <text x="60" y="310" font-size="11">• Mutex contention</text>

  <text x="310" y="270" font-size="11" fill="#388e3c">✓ Non-blocking receive</text>
  <text x="310" y="290" font-size="11" fill="#388e3c">✓ Butter-smooth!</text>
  <text x="310" y="310" font-size="11" fill="#388e3c">✓ Lock-free</text>
</svg>
```

### Layer 3: Double Buffering

**Problem:** Within Core 1, display refresh (200Hz) and render task compete for framebuffer access.

**Solution:** Two separate framebuffers with different mutexes:
- Display refresh reads from **active** buffer (fb0 or fb1)
- Render task writes to **inactive** buffer (fb1 or fb0)
- Atomic swap when render complete (~1 instruction)

```svg
<svg viewBox="0 0 700 400" xmlns="http://www.w3.org/2000/svg">
  <text x="350" y="30" text-anchor="middle" font-size="18" font-weight="bold">
    Double Buffering (Core 1)
  </text>

  <!-- Timeline -->
  <text x="50" y="80" font-size="14" font-weight="bold">Frame N:</text>

  <!-- Display reads FB0 -->
  <rect x="150" y="60" width="150" height="60" fill="#c8e6c9" stroke="#388e3c" stroke-width="2" rx="5"/>
  <text x="225" y="85" text-anchor="middle" font-size="12" font-weight="bold">Display Refresh</text>
  <text x="225" y="105" text-anchor="middle" font-size="11">Reads FB0 (2ms)</text>

  <!-- Render writes FB1 -->
  <rect x="350" y="60" width="150" height="60" fill="#ffccbc" stroke="#d84315" stroke-width="2" rx="5"/>
  <text x="425" y="85" text-anchor="middle" font-size="12" font-weight="bold">Render Task</text>
  <text x="425" y="105" text-anchor="middle" font-size="11">Writes FB1 (15ms)</text>

  <text x="550" y="95" font-size="14" fill="#388e3c">✓ No blocking!</text>

  <!-- Atomic Swap -->
  <rect x="250" y="150" width="200" height="40" fill="#fff9c4" stroke="#f57f17" stroke-width="2" rx="5"/>
  <text x="350" y="175" text-anchor="middle" font-size="12" font-weight="bold">
    ⚡ Atomic Swap (~1 instruction)
  </text>

  <!-- Timeline -->
  <text x="50" y="230" font-size="14" font-weight="bold">Frame N+1:</text>

  <!-- Display reads FB1 (swapped!) -->
  <rect x="150" y="210" width="150" height="60" fill="#c8e6c9" stroke="#388e3c" stroke-width="2" rx="5"/>
  <text x="225" y="235" text-anchor="middle" font-size="12" font-weight="bold">Display Refresh</text>
  <text x="225" y="255" text-anchor="middle" font-size="11">Reads FB1 (2ms)</text>

  <!-- Render writes FB0 (swapped!) -->
  <rect x="350" y="210" width="150" height="60" fill="#ffccbc" stroke="#d84315" stroke-width="2" rx="5"/>
  <text x="425" y="235" text-anchor="middle" font-size="12" font-weight="bold">Render Task</text>
  <text x="425" y="255" text-anchor="middle" font-size="11">Writes FB0 (15ms)</text>

  <text x="550" y="245" font-size="14" fill="#388e3c">✓ Still no blocking!</text>

  <!-- Key insight box -->
  <rect x="100" y="310" width="500" height="70" fill="#e8f5e9" stroke="#388e3c" stroke-width="2" rx="5"/>
  <text x="350" y="335" text-anchor="middle" font-size="13" font-weight="bold" fill="#388e3c">
    🔑 Key Insight: Different Mutexes = No Contention
  </text>
  <text x="350" y="355" text-anchor="middle" font-size="11">
    Mutex(FB0) and Mutex(FB1) are separate - tasks never block each other!
  </text>
  <text x="350" y="370" text-anchor="middle" font-size="11">
    Display can lock FB0 while Render locks FB1 simultaneously
  </text>
</svg>
```

## Memory Layout

```svg
<svg viewBox="0 0 600 500" xmlns="http://www.w3.org/2000/svg">
  <text x="300" y="30" text-anchor="middle" font-size="18" font-weight="bold">
    Memory Layout (ESP32-S3 Matrix Portal)
  </text>

  <!-- Internal SRAM -->
  <rect x="50" y="60" width="200" height="100" fill="#e3f2fd" stroke="#1976d2" stroke-width="2" rx="5"/>
  <text x="150" y="85" text-anchor="middle" font-size="14" font-weight="bold">Internal SRAM (~512KB)</text>
  <text x="150" y="105" text-anchor="middle" font-size="11">Heap: 120KB</text>
  <text x="150" y="120" text-anchor="middle" font-size="11">WiFi/Radio: ~80KB</text>
  <text x="150" y="135" text-anchor="middle" font-size="11">Stack/System: ~312KB</text>
  <text x="150" y="150" text-anchor="middle" font-size="11" fill="#1976d2">Fast access!</text>

  <!-- PSRAM -->
  <rect x="300" y="60" width="250" height="400" fill="#fff9c4" stroke="#f57f17" stroke-width="3" rx="5"/>
  <text x="425" y="85" text-anchor="middle" font-size="14" font-weight="bold">PSRAM (2MB External)</text>

  <!-- Framebuffer 0 -->
  <rect x="320" y="100" width="210" height="70" fill="#ffccbc" stroke="#d84315" stroke-width="2" rx="5"/>
  <text x="425" y="125" text-anchor="middle" font-size="12" font-weight="bold">Framebuffer 0</text>
  <text x="425" y="145" text-anchor="middle" font-size="11">32×128×1-bit + DMA</text>
  <text x="425" y="160" text-anchor="middle" font-size="11">~200KB</text>

  <!-- Framebuffer 1 -->
  <rect x="320" y="185" width="210" height="70" fill="#ffccbc" stroke="#d84315" stroke-width="2" rx="5"/>
  <text x="425" y="210" text-anchor="middle" font-size="12" font-weight="bold">Framebuffer 1</text>
  <text x="425" y="230" text-anchor="middle" font-size="11">32×128×1-bit + DMA</text>
  <text x="425" y="245" text-anchor="middle" font-size="11">~200KB</text>

  <!-- API Buffer -->
  <rect x="320" y="270" width="210" height="60" fill="#c8e6c9" stroke="#388e3c" stroke-width="2" rx="5"/>
  <text x="425" y="295" text-anchor="middle" font-size="12" font-weight="bold">API Response Buffer</text>
  <text x="425" y="315" text-anchor="middle" font-size="11">256KB (heap allocated)</text>

  <!-- Free space -->
  <rect x="320" y="345" width="210" height="100" fill="#f5f5f5" stroke="#666" stroke-width="1" stroke-dasharray="5,5" rx="5"/>
  <text x="425" y="380" text-anchor="middle" font-size="12" font-weight="bold">Free PSRAM</text>
  <text x="425" y="400" text-anchor="middle" font-size="11">~1.35MB available</text>
  <text x="425" y="420" text-anchor="middle" font-size="10" fill="#666">Ready for 3rd screen!</text>

  <!-- Flash -->
  <rect x="50" y="180" width="200" height="80" fill="#e0e0e0" stroke="#616161" stroke-width="2" rx="5"/>
  <text x="150" y="205" text-anchor="middle" font-size="14" font-weight="bold">Flash (8MB)</text>
  <text x="150" y="225" text-anchor="middle" font-size="11">Program Code: 633KB</text>
  <text x="150" y="240" text-anchor="middle" font-size="11">Free: 7.4MB</text>
  <text x="150" y="255" text-anchor="middle" font-size="11" fill="#666">(Read-only at runtime)</text>
</svg>
```

## Data Flow

The complete journey of metro departure data through the system:

```svg
<svg viewBox="0 0 800 700" xmlns="http://www.w3.org/2000/svg">
  <text x="400" y="30" text-anchor="middle" font-size="20" font-weight="bold">
    Data Flow: API → Display
  </text>

  <defs>
    <marker id="flow-arrow" markerWidth="10" markerHeight="10" refX="9" refY="3" orient="auto">
      <polygon points="0 0, 10 3, 0 6" fill="#333" />
    </marker>
  </defs>

  <!-- Step 1: HTTP Request -->
  <rect x="50" y="60" width="200" height="80" fill="#e3f2fd" stroke="#1976d2" stroke-width="2" rx="5"/>
  <text x="150" y="85" text-anchor="middle" font-size="13" font-weight="bold">1. HTTP Request (Core 0)</text>
  <text x="150" y="105" text-anchor="middle" font-size="11">GET /v1/sites/9112/departures</text>
  <text x="150" y="120" text-anchor="middle" font-size="10" fill="#666">WiFi + TLS + DNS</text>
  <text x="150" y="133" text-anchor="middle" font-size="10" fill="#666">~500ms</text>

  <!-- Arrow down -->
  <path d="M 150 140 L 150 170" fill="none" stroke="#333" stroke-width="2" marker-end="url(#flow-arrow)"/>

  <!-- Step 2: JSON Parsing -->
  <rect x="50" y="170" width="200" height="80" fill="#ffccbc" stroke="#d84315" stroke-width="2" rx="5"/>
  <text x="150" y="195" text-anchor="middle" font-size="13" font-weight="bold">2. JSON Parsing (Core 0)</text>
  <text x="150" y="215" text-anchor="middle" font-size="11">256KB buffer in PSRAM</text>
  <text x="150" y="230" text-anchor="middle" font-size="10" fill="#666">serde_json_core</text>
  <text x="150" y="243" text-anchor="middle" font-size="10" fill="#666">~50ms</text>

  <!-- Arrow right -->
  <path d="M 250 210 L 340 210" fill="none" stroke="#388e3c" stroke-width="3" marker-end="url(#flow-arrow)"/>
  <text x="295" y="205" text-anchor="middle" font-size="11" fill="#388e3c" font-weight="bold">Channel</text>

  <!-- Step 3: Channel Send -->
  <rect x="340" y="170" width="200" height="80" fill="#c8e6c9" stroke="#388e3c" stroke-width="2" rx="5"/>
  <text x="440" y="195" text-anchor="middle" font-size="13" font-weight="bold">3. Channel.send()</text>
  <text x="440" y="215" text-anchor="middle" font-size="11">Vec&lt;(line, dest, time)&gt;</text>
  <text x="440" y="230" text-anchor="middle" font-size="10" fill="#666">Non-blocking</text>
  <text x="440" y="243" text-anchor="middle" font-size="10" fill="#388e3c">Lock-free! ✓</text>

  <!-- Arrow down -->
  <path d="M 440 250 L 440 280" fill="none" stroke="#333" stroke-width="2" marker-end="url(#flow-arrow)"/>

  <!-- Core boundary -->
  <line x1="20" y1="270" x2="780" y2="270" stroke="#7b1fa2" stroke-width="3" stroke-dasharray="10,5"/>
  <text x="700" y="265" font-size="12" fill="#7b1fa2" font-weight="bold">→ Core 1</text>

  <!-- Step 4: Channel Receive -->
  <rect x="340" y="290" width="200" height="80" fill="#f3e5f5" stroke="#7b1fa2" stroke-width="2" rx="5"/>
  <text x="440" y="315" text-anchor="middle" font-size="13" font-weight="bold">4. try_receive() (Core 1)</text>
  <text x="440" y="335" text-anchor="middle" font-size="11">Check every 10ms</text>
  <text x="440" y="350" text-anchor="middle" font-size="10" fill="#666">Non-blocking poll</text>
  <text x="440" y="363" text-anchor="middle" font-size="10" fill="#7b1fa2">100Hz render loop</text>

  <!-- Arrow down -->
  <path d="M 440 370 L 440 400" fill="none" stroke="#333" stroke-width="2" marker-end="url(#flow-arrow)"/>

  <!-- Step 5: Render to Back Buffer -->
  <rect x="340" y="400" width="200" height="100" fill="#fff9c4" stroke="#f57f17" stroke-width="2" rx="5"/>
  <text x="440" y="425" text-anchor="middle" font-size="13" font-weight="bold">5. Render to Back Buffer</text>
  <text x="440" y="445" text-anchor="middle" font-size="11">Lock inactive FB (fb0 or fb1)</text>
  <text x="440" y="460" text-anchor="middle" font-size="10" fill="#666">Clear + draw text</text>
  <text x="440" y="475" text-anchor="middle" font-size="10" fill="#666">Calculate scroll offset</text>
  <text x="440" y="490" text-anchor="middle" font-size="10" fill="#f57f17">~10-20ms</text>

  <!-- Arrow down -->
  <path d="M 440 500 L 440 530" fill="none" stroke="#333" stroke-width="2" marker-end="url(#flow-arrow)"/>

  <!-- Step 6: Atomic Swap -->
  <rect x="340" y="530" width="200" height="70" fill="#c8e6c9" stroke="#388e3c" stroke-width="2" rx="5"/>
  <text x="440" y="555" text-anchor="middle" font-size="13" font-weight="bold">6. Atomic Buffer Swap</text>
  <text x="440" y="575" text-anchor="middle" font-size="11">active_is_zero.fetch_xor(true)</text>
  <text x="440" y="590" text-anchor="middle" font-size="10" fill="#388e3c">⚡ Instant! (~1 instruction)</text>

  <!-- Arrow down -->
  <path d="M 440 600 L 440 630" fill="none" stroke="#333" stroke-width="2" marker-end="url(#flow-arrow)"/>

  <!-- Step 7: Display Refresh -->
  <rect x="340" y="630" width="200" height="60" fill="#e1f5fe" stroke="#0277bd" stroke-width="2" rx="5"/>
  <text x="440" y="655" text-anchor="middle" font-size="13" font-weight="bold">7. Display Refresh</text>
  <text x="440" y="675" text-anchor="middle" font-size="11">DMA from active buffer @ 200Hz</text>

  <!-- Parallel display refresh indicator -->
  <rect x="580" y="290" width="180" height="400" fill="#e1f5fe" stroke="#0277bd" stroke-width="2" stroke-dasharray="5,5" rx="5"/>
  <text x="670" y="315" text-anchor="middle" font-size="12" font-weight="bold" fill="#0277bd">Display Refresh</text>
  <text x="670" y="335" text-anchor="middle" font-size="11" fill="#0277bd">(Always Running)</text>

  <!-- Timing bars -->
  <rect x="590" y="350" width="20" height="40" fill="#0277bd"/>
  <rect x="620" y="350" width="20" height="40" fill="#0277bd"/>
  <rect x="650" y="350" width="20" height="40" fill="#0277bd"/>
  <rect x="680" y="350" width="20" height="40" fill="#0277bd"/>
  <rect x="710" y="350" width="20" height="40" fill="#0277bd"/>
  <text x="670" y="410" text-anchor="middle" font-size="10">Every 5ms (200Hz)</text>

  <text x="670" y="450" text-anchor="middle" font-size="11" fill="#388e3c">
    Reads from front buffer
  </text>
  <text x="670" y="470" text-anchor="middle" font-size="11" fill="#d84315">
    While render writes to back
  </text>
  <text x="670" y="495" text-anchor="middle" font-size="12" font-weight="bold" fill="#388e3c">
    ✓ No blocking!
  </text>
  <text x="670" y="515" text-anchor="middle" font-size="12" font-weight="bold" fill="#388e3c">
    ✓ Butter-smooth!
  </text>
</svg>
```

## Performance Characteristics

### Task Frequencies
- **Display Refresh:** 200Hz (every 5ms) - High priority, never blocks
- **Render+Scroll:** 100Hz (every 10ms) - Low priority, can take time
- **API Fetch:** Every 20 seconds - Runs on Core 0, doesn't affect display

### Memory Usage
- **Total PSRAM Used:** ~650KB / 2MB (32% utilization)
- **Framebuffer 0:** ~200KB
- **Framebuffer 1:** ~200KB
- **API Buffer:** 256KB (dynamically allocated)
- **Free PSRAM:** ~1.35MB (ready for expansion!)

### Why It's Smooth

1. **Core isolation:** Network/parsing on Core 0, display on Core 1
2. **Lock-free channel:** No cross-core mutex contention
3. **Double buffering:** Display and render never compete for same buffer
4. **Atomic swap:** Buffer switch is ~1 instruction (instant)
5. **Priority scheduling:** Display refresh is high priority on Core 1

## Code Structure

### Key Files

- `src/bin/main.rs`: Core initialization, task spawning, dual-core setup
- `src/api.rs`: HTTP client, JSON parsing (uses 256KB PSRAM buffer)
- `src/display.rs`: Framebuffer operations, text rendering
- `src/types.rs`: Shared data structures (DepartureData)
- `config.toml`: Runtime configuration (WiFi, API, display settings)

### Important Types

```rust
// Departure data shared via channel
type DepartureData = Vec<
    (
        heapless::String<8>,   // line number
        heapless::String<32>,  // destination
        heapless::String<16>,  // time
    ),
    10,
>;

// Framebuffer type (in PSRAM)
type DisplayFrameBuffer = DmaFrameBuffer<ROWS, COLS, NROWS, BITS, FRAME_COUNT>;

// Lock-free channel for Core 0 → Core 1
Channel<CriticalSectionRawMutex, DepartureData, 1>
```

## Extending the System

### Adding a Third Screen

With 1.35MB free PSRAM, adding a 3rd screen is straightforward:

1. Update `config.toml`:
   ```toml
   [display]
   num_screens = 3  # Change from 2 to 3
   ```

2. Rebuild and flash:
   ```bash
   cargo build --release
   cargo run --release
   ```

The framebuffers will automatically resize to 32×192 pixels (3 screens).

### Memory Impact
- Additional framebuffer space: ~100KB per buffer × 2 = 200KB
- Still leaves ~1.15MB PSRAM free
- No code changes needed!

## Debugging

### Enable Detailed Logging

```bash
ESP_LOG=debug cargo run --release
```

This shows:
- Frame render counts
- Scroll offset updates
- Channel send/receive events
- DMA transfer timing

### Key Log Messages

- `"Display refresh task started (double buffered, safe Rust!)"` - Core 1 display task running
- `"Render+Scroll task started on Core 1 (double buffered, lock-free channel!)"` - Core 1 render task running
- `"Data fetch task started (using lock-free channel)"` - Core 0 API task running
- `"Fetched 6 departures"` - Successful API response
- `"Sent departure data to channel"` - Data passed to Core 1
- `"Received 6 departures from channel"` - Core 1 got new data

## References

- [ESP32-S3 Datasheet](https://www.espressif.com/sites/default/files/documentation/esp32-s3_datasheet_en.pdf)
- [HUB75 Protocol](https://learn.adafruit.com/32x16-32x32-rgb-led-matrix/overview)
- [Embassy Async Runtime](https://embassy.dev/)
- [ESP-RS Book](https://docs.esp-rs.org/)
