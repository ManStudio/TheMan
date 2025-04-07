	last: ?Ticket
	conversation: Ticket
	time: Time
	data: []U8
`last` can be [[Ticket]] to the last message from this conversation or other.
`conversation` Is a [[Ticket]] to the [[Signed]] [[Raw Conversation]].
`time` Is the time when the message was created.
`data` Is [UTF-8](https://en.wikipedia.org/wiki/UTF-8) if not encrypted.

By default no message will be encrypted.

The Raw Message will always be stored [[#Signed]] in the network.