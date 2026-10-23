#!/usr/bin/env node
const net = require('net');
const iconv = require('iconv-lite');

const state = {
  leases: [],
  options: [],
  arp: [
    { id: '*1', address: '192.168.88.42', macAddress: 'AA:BB:CC:DD:EE:FF', interface: 'ether1' }
  ]
};

function encodeLength(len) {
  if (len < 0x80) {
    return Buffer.from([len]);
  } else if (len < 0x4000) {
    let l = len | 0x8000;
    return Buffer.from([(l >> 8) & 0xff, l & 0xff]);
  } else if (len < 0x200000) {
    let l = len | 0xc00000;
    return Buffer.from([(l >> 16) & 0xff, (l >> 8) & 0xff, l & 0xff]);
  } else {
    let l = len | 0xe0000000;
    return Buffer.from([(l >> 24) & 0xff, (l >> 16) & 0xff, (l >> 8) & 0xff, l & 0xff]);
  }
}

function encodeSentence(s) {
  if (s === null || s === undefined) {
    return Buffer.from([0x00]);
  }
  const encoded = iconv.encode(s, 'win1252');
  return Buffer.concat([encodeLength(encoded.length), encoded]);
}

function decodeLength(buf, offset) {
  let idx = offset;
  const b = buf[idx++];
  if (b & 0x80) {
    if ((b & 0xc0) === 0x80) {
      const len = ((b & 0x3f) << 8) + buf[idx++];
      return { len, bytes: idx - offset };
    } else if ((b & 0xe0) === 0xc0) {
      let len = ((b & 0x1f) << 8) + buf[idx++];
      len = (len << 8) + buf[idx++];
      return { len, bytes: idx - offset };
    } else if ((b & 0xf0) === 0xe0) {
      let len = ((b & 0x0f) << 8) + buf[idx++];
      len = (len << 8) + buf[idx++];
      len = (len << 8) + buf[idx++];
      return { len, bytes: idx - offset };
    } else {
      let len = buf[idx++];
      len = (len << 8) + buf[idx++];
      len = (len << 8) + buf[idx++];
      len = (len << 8) + buf[idx++];
      return { len, bytes: idx - offset };
    }
  } else {
    return { len: b, bytes: 1 };
  }
}

function readAllSentences(buf) {
  const sentences = [];
  let offset = 0;
  while (offset < buf.length) {
    if (buf[offset] === 0x00) {
      sentences.push(null);
      offset++;
      continue;
    }
    const { len, bytes: lenBytes } = decodeLength(buf, offset);
    const start = offset + lenBytes;
    const sentence = iconv.decode(buf.slice(start, start + len), 'win1252');
    sentences.push(sentence);
    offset = start + len;
  }
  return sentences;
}

const server = net.createServer((socket) => {
  console.log('Client connected');
  let buffer = Buffer.alloc(0);
  
  socket.on('data', (data) => {
    buffer = Buffer.concat([buffer, data]);
    // Check if we have a complete command (ends with null)
    if (buffer.indexOf(0x00) === -1) return;
    
    const sentences = readAllSentences(buffer);
    buffer = Buffer.alloc(0);
    
    const parsed = sentences.map(s => {
      if (s === null) return { type: 'null' };
      if (typeof s === 'string' && s.startsWith('.tag=')) return { type: 'tag', value: s.slice(5) };
      if (typeof s === 'string' && (s.startsWith('=') || s.startsWith('?'))) {
        const idx = s.indexOf('=');
        if (idx >= 0) return { type: 'attr', key: s.slice(1, idx), value: s.slice(idx + 1) };
      }
      if (typeof s === 'string') return { type: 'cmd', value: s };
      return { type: 'unknown' };
    });
    
    const tag = parsed.find(p => p.type === 'tag')?.value || '';
    const cmd = parsed.find(p => p.type === 'cmd')?.value || '';
    const attrs = parsed.filter(p => p.type === 'attr');
    
    console.log('CMD:', cmd, 'tag:', tag);
    
    const resp = [];
    if (cmd === '/login') {
      resp.push(encodeSentence('!done'));
      resp.push(encodeSentence('.tag=' + tag));
      resp.push(encodeSentence(null));
      socket.write(Buffer.concat(resp));
      return;
    }
    
    if (cmd === '/ip/dhcp-server/lease/print') {
      let results = state.leases;
      for (const a of attrs) {
        if (a.key === 'address') results = state.leases.filter(l => l.address === a.value);
      }
      if (results.length === 0) {
        resp.push(encodeSentence('!empty'));
        resp.push(encodeSentence('.tag=' + tag));
        resp.push(encodeSentence(null));
      } else {
        for (const r2 of results) {
          resp.push(encodeSentence('!re'));
          resp.push(encodeSentence('=.id=' + r2.id));
          resp.push(encodeSentence('=address=' + r2.address));
          resp.push(encodeSentence('=mac-address=' + r2.macAddress));
          resp.push(encodeSentence('=comment=' + (r2.comment || '')));
          resp.push(encodeSentence('=dynamic=' + (r2.dynamic ? 'true' : 'false')));
          resp.push(encodeSentence('=dhcp-option=' + (r2.dhcpOption || '')));
          resp.push(encodeSentence('.tag=' + tag));
          resp.push(encodeSentence(null));
        }
      }
      socket.write(Buffer.concat(resp));
      return;
    }
    
    if (cmd === '/ip/dhcp-server/lease/add') {
      const lease = { id: '*' + (state.leases.length + 1), address: '', macAddress: '', comment: '', dynamic: false, dhcpOption: '' };
      for (const a of attrs) {
        if (a.key === 'address') lease.address = a.value;
        if (a.key === 'mac-address') lease.macAddress = a.value;
        if (a.key === 'dhcp-option') lease.dhcpOption = a.value;
        if (a.key === 'comment') lease.comment = a.value;
      }
      state.leases.push(lease);
      resp.push(encodeSentence('!done'));
      resp.push(encodeSentence('=ret=' + lease.id));
      resp.push(encodeSentence('.tag=' + tag));
      resp.push(encodeSentence(null));
      socket.write(Buffer.concat(resp));
      return;
    }
    
    if (cmd === '/ip/dhcp-server/lease/remove') {
      const ids = [];
      for (const a of attrs) {
        if (a.key === 'numbers') ids.push(...a.value.split(','));
      }
      state.leases = state.leases.filter(l => !ids.includes(l.id));
      resp.push(encodeSentence('!done'));
      resp.push(encodeSentence('.tag=' + tag));
      resp.push(encodeSentence(null));
      socket.write(Buffer.concat(resp));
      return;
    }
    
    if (cmd === '/ip/arp/print') {
      let results = state.arp;
      for (const a of attrs) {
        if (a.key === 'address') results = state.arp.filter(a2 => a2.address === a.value);
      }
      if (results.length === 0) {
        resp.push(encodeSentence('!empty'));
        resp.push(encodeSentence('.tag=' + tag));
        resp.push(encodeSentence(null));
      } else {
        for (const r2 of results) {
          resp.push(encodeSentence('!re'));
          resp.push(encodeSentence('=.id=' + r2.id));
          resp.push(encodeSentence('=address=' + r2.address));
          resp.push(encodeSentence('=mac-address=' + r2.macAddress));
          resp.push(encodeSentence('=interface=' + r2.interface));
          resp.push(encodeSentence('.tag=' + tag));
          resp.push(encodeSentence(null));
        }
      }
      socket.write(Buffer.concat(resp));
      return;
    }
    
    if (cmd === '/ip/dhcp-server/option/print') {
      let results = state.options;
      for (const a of attrs) {
        if (a.key === 'name') results = state.options.filter(o => o.name === a.value);
      }
      if (results.length === 0) {
        resp.push(encodeSentence('!empty'));
        resp.push(encodeSentence('.tag=' + tag));
        resp.push(encodeSentence(null));
      } else {
        for (const r2 of results) {
          resp.push(encodeSentence('!re'));
          resp.push(encodeSentence('=.id=' + r2.id));
          resp.push(encodeSentence('=name=' + r2.name));
          resp.push(encodeSentence('=code=' + r2.code));
          resp.push(encodeSentence('=value=' + r2.value));
          if (r2.comment) resp.push(encodeSentence('=comment=' + r2.comment));
          resp.push(encodeSentence('.tag=' + tag));
          resp.push(encodeSentence(null));
        }
      }
      socket.write(Buffer.concat(resp));
      return;
    }
    
    if (cmd === '/ip/dhcp-server/option/add') {
      const opt = { id: '*' + (state.options.length + 1), name: '', code: '', value: '', comment: '' };
      for (const a of attrs) {
        if (a.key === 'name') opt.name = a.value;
        if (a.key === 'code') opt.code = a.value;
        if (a.key === 'value') opt.value = a.value;
        if (a.key === 'comment') opt.comment = a.value;
      }
      state.options.push(opt);
      resp.push(encodeSentence('!done'));
      resp.push(encodeSentence('=ret=' + opt.id));
      resp.push(encodeSentence('.tag=' + tag));
      resp.push(encodeSentence(null));
      socket.write(Buffer.concat(resp));
      return;
    }
    
    resp.push(encodeSentence('!done'));
    resp.push(encodeSentence('.tag=' + tag));
    resp.push(encodeSentence(null));
    socket.write(Buffer.concat(resp));
  });
  
  socket.on('end', () => console.log('Client disconnected'));
});

const PORT = process.env.MOCK_PORT || 8728;
server.listen(PORT, () => {
  console.log(`Mock MikroTik API listening on port ${PORT}`);
});
