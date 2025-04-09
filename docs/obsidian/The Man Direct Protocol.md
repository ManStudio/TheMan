---
tags:
  - project/the-man
  - protocol/tmd
---
This is a protocol used for real time audio/video communication.

# Packets

## Start
	conversation_hash: [32]U8
	idx: U32
	codec: String
	settings: String
## Play
	conversation_hash: [32]U8
	idx: U32
	data: []U8
## Stop
	conversation_hash: [32]U8
	idx: U32