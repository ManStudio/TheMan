---
tags:
  - project/the-man
  - protocol/tmm
---
This is a protocol used to send, receive and sync messages.
Message data always will be valid UTF-8

This is the abstract way on how the protocol works.
This protocol needs other protocol to fetch the data for the hashes, [Iroh Blobs](https://docs.rs/iroh-blobs/0.31.0/iroh_blobs/index.html) is used.
## Components

### Signed
	data: []u8
	signature: signature

If is got from a [[Ticket]] the ticket owner is used to verify the data.
The data can be a [[#Conversation]] or a [[#Message]]
### Conversation
	nodes: []NodeId
	time: time

`nodes` will be the nodes that are in this conversation.
`time` will be the time when the conversation was created.
The [[#Conversation]] will always be stored [[#Signed]] in the network.
### Message
	last: ?Ticket
	conversation: Ticket
	time: time
	data: String

`last` is a ticket to the [[#Signed]] [[#Message]] construct.
`conversation` Is a ticket to the [[#Signed]] [[#Conversation]] construct.
`time` Is the time when the message was created.
`data` Is the message data always UTF-8 if not is considered corrupted.
The [[#Message]] will always be stored [[#Signed]] in the network.

## Packets
This is what is sent to the other nodes.
### Welcome

This is treated as a sync packet.
When we receive this packet we will search in every conversation that the other node is part of and send the last messages [[Ticket]]s
### Send Message
	ticket: Ticket

When we receive this packet we will try to download the ticket data, then will we verify the [[#Signed]] data with the ticket owner, then we will get the conversation from the message.conversation then the same process for verifying it and if the message owner is part of that conversation we will add the message to that conversation.