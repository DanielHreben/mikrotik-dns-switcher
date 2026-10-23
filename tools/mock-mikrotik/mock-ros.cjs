#!/usr/bin/env node
const net = require('net');
const iconv = require('iconv-lite');

// Simple in-memory state
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

function readSentence(buf, offset) {
  if (offset >= buf.length) return { sentence: undefined, bytes: 0 };
  if (buf[offset] === 0x00) {
    return { sentence: null, bytes: 1 };
  }
  const { len, bytes: lenBytes } = decodeLength(buf, offset);
  if (offset + lenBytes + len > buf.length) {
    return { sentence: undefined, bytes: 0 };
  }
  const start = offset + lenBytes;
  const sentence = iconv.decode(buf.slice(start, start + len), 'win1252');
  return { sentence, bytes: lenBytes + len };
}

function parseSentence(s) {
  if (s.startsWith('=') || s.startsWith('?')) {
    const idx = s.indexOf('=');
    if (idx >= 0) {
      return { type: 'attr', key: s.slice(1, idx), value: s.slice(idx + 1), query: s.startsWith('?') };
    }
  } else if (s.startsWith('.tag=')) {
    return { type: 'tag', value: s.slice(5) };
  }
  return { type: 'cmd', value: s };
}

function handleLogin(sentences, tag) {
  const replies = [];
  replies.push(encodeSentence('!done'));
  replies.push(encodeSentence('.tag=' + tag));
  replies.push(encodeSentence(null));
  return Buffer.concat(replies);
}

function handleLeasePrint(attrs, tag) {
  const replies = [];
  let results = state.leases;
  for (const a of attrs) {
    if (a.key === 'address') {
      results = state.leases.filter(l => l.address === a.value);
    }
  }
  for (const r of results) {
    replies.push(encodeSentence('!re'));
    for (const [k, v] of Object.entries(r)) {
      if (k === 'id') {
        replies.push(encodeSentence('=.id=' + v));
      } else if (k === 'macAddress') {
        replies.push(encodeSentence('=mac-address=' + v));
      } else if (k === 'dhcpOption') {
        replies.push(encodeSentence('=dhcp-option=' + v));
      } else if (typeof v === 'boolean') {
        replies.push(encodeSentence('=' + k.replace(/([A-Z])/g, '-$1').toLowerCase() + '=' + (v ? 'true' : 'false')));
      } else if (v !== undefined && v !== null) {
        replies.push(encodeSentence('=' + k.replace(/([A-Z])/g, '-$1').toLowerCase() + '=' + v));
      }
    }
    replies.push(encodeSentence('.tag=' + tag));
    replies.push(encodeSentence(null));
  }
  if (results.length === 0) {
    replies.push(encodeSentence('!empty'));
    replies.push(encodeSentence('.tag=' + tag));
    replies.push(encodeSentence(null));
  }
  return Buffer.concat(replies);
}

function handleLeaseAdd(attrs, tag) {
  const lease = {
    id: '*' + (state.leases.length + 1),
    address: '',
    macAddress: '',
    comment: '',
    dynamic: false,
    disabled: false,
    blocked: false,
    dhcpOption: ''
  };
  for (const a of attrs) {
    if (a.key === 'address') lease.address = a.value;
    if (a.key === 'mac-address') lease.macAddress = a.value;
    if (a.key === 'dhcp-option') lease.dhcpOption = a.value;
    if (a.key === 'comment') lease.comment = a.value;
  }
  state.leases.push(lease);
  const replies = [];
  replies.push(encodeSentence('!done'));
  replies.push(encodeSentence('=ret=' + lease.id));
  replies.push(encodeSentence('.tag=' + tag));
  replies.push(encodeSentence(null));
  return Buffer.concat(replies);
}

function handleLeaseRemove(attrs, tag) {
  const ids = [];
  for (const a of attrs) {
    if (a.key === 'numbers') ids.push(...a.value.split(','));
  }
  state.leases = state.leases.filter(l => !ids.includes(l.id));
  const replies = [];
  replies.push(encodeSentence('!done'));
  replies.push(encodeSentence('.tag=' + tag));
  replies.push(encodeSentence(null));
  return Buffer.concat(replies);
}

function handleArpPrint(attrs, tag) {
  const replies = [];
  let results = state.arp;
  for (const a of attrs) {
    if (a.key === 'address') {
      results = state.arp.filter(a2 => a2.address === a.value);
    }
  }
  for (const r of results) {
    replies.push(encodeSentence('!re'));
    replies.push(encodeSentence('=.id=' + r.id));
    replies.push(encodeSentence('=address=' + r.address));
    replies.push(encodeSentence('=mac-address=' + r.macAddress));
    replies.push(encodeSentence('=interface=' + r.interface));
    replies.push(encodeSentence('.tag=' + tag));
    replies.push(encodeSentence(null));
  }
  if (results.length === 0) {
    replies.push(encodeSentence('!empty'));
    replies.push(encodeSentence('.tag=' + tag));
    replies.push(encodeSentence(null));
  }
  return Buffer.concat(replies);
}

function handleDhcpOptionPrint(attrs, tag) {
  const replies = [];
  let results = state.options;
  for (const a of attrs) {
    if (a.key === 'name') {
      results = state.options.filter(o => o.name === a.value);
    }
  }
  for (const r of results) {
    replies.push(encodeSentence('!re'));
    replies.push(encodeSentence('=.id=' + r.id));
    replies.push(encodeSentence('=name=' + r.name));
    replies.push(encodeSentence('=code=' + r.code));
    replies.push(encodeSentence('=value=' + r.value));
    if (r.comment) replies.push(encodeSentence('=comment=' + r.comment));
    replies.push(encodeSentence('.tag=' + tag));
    replies.push(encodeSentence(null));
  }
  if (results.length === 0) {
    replies.push(encodeSentence('!empty'));
    replies.push(encodeSentence('.tag=' + tag));
    replies.push(encodeSentence(null));
  }
  return Buffer.concat(replies);
}

function handleDhcpOptionAdd(attrs, tag) {
  const opt = { id: '*' + (state.options.length + 1), name: '', code: '', value: '', comment: '' };
  for (const a of attrs) {
    if (a.key === 'name') opt.name = a.value;
    if (a.key === 'code') opt.code = a.value;
    if (a.key === 'value') opt.value = a.value;
    if (a.key === 'comment') opt.comment = a.value;
  }
  state.options.push(opt);
  const replies = [];
  replies.push(encodeSentence('!done'));
  replies.push(encodeSentence('=ret=' + opt.id));
  replies.push(encodeSentence('.tag=' + tag));
  replies.push(encodeSentence(null));
  return Buffer.concat(replies);
}

const server = net.createServer((socket) => {
  console.log('Client connected');
  let buffer = Buffer.alloc(0);
  
  socket.on('data', (data) => {
    buffer = Buffer.concat([buffer, data]);
    let offset = 0;
    while (offset < buffer.length) {
      const result = readSentence(buffer, offset);
      if (result.sentence === undefined) break;
      if (result.sentence === null) {
        const sentences = [];
        // Need to collect all sentences up to this point - simpler approach
        let o2 = 0;
        const allSentences = [];
        while (o2 < offset) {
          const r2 = readSentence(buffer, o2);
          if (r2.sentence === undefined) break;
          if (r2.sentence === null) {
            o2 += r2.bytes;
            break;
          }
          allSentences.push(r2.sentence);
          o2 += r2.bytes;
        }
        const cmd = allSentences.map(parseSentence);
        const tag = cmd.find(c => c.type === 'tag')?.value;
        const cmdLine = cmd.find(c => c.type === 'cmd')?.value;
        const attrs = cmd.filter(c => c.type === 'attr');
        
        console.log('Command:', cmdLine, 'tag:', tag);
        
        let response;
        if (cmdLine === '/login') {
          response = handleLogin(cmd, tag);
        } else if (cmdLine === '/ip/dhcp-server/lease/print') {
          response = handleLeasePrint(attrs, tag);
        } else if (cmdLine === '/ip/dhcp-server/lease/add') {
          response = handleLeaseAdd(attrs, tag);
        } else if (cmdLine === '/ip/dhcp-server/lease/remove') {
          response = handleLeaseRemove(attrs, tag);
        } else if (cmdLine === '/ip/arp/print') {
          response = handleArpPrint(attrs, tag);
        } else if (cmdLine === '/ip/dhcp-server/option/print') {
          response = handleDhcpOptionPrint(attrs, tag);
        } else if (cmdLine === '/ip/dhcp-server/option/add') {
          response = handleDhcpOptionAdd(attrs, tag);
        } else {
          const replies = [];
          replies.push(encodeSentence('!done'));
          replies.push(encodeSentence('.tag=' + tag));
          replies.push(encodeSentence(null));
          response = Buffer.concat(replies);
        }
        
        socket.write(response);
        offset += result.bytes;
        buffer = buffer.slice(offset);
        offset = 0;
        continue;
      } else {
        offset += result.bytes;
      }
    }
  });
  
  socket.on('end', () => {
    console.log('Client disconnected');
  });
});

const PORT = process.env.MOCK_PORT || 8728;
server.listen(PORT, () => {
  console.log(`Mock MikroTik API listening on port ${PORT}`);
});
