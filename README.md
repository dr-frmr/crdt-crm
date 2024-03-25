### A simple CRDT app on Kinode

Uses [automerge](https://automerge.org/automerge/automerge/) and [autosurgeon](https://docs.rs/autosurgeon/latest/autosurgeon/index.html) to create a simple CRDT "CRM" app.

To build, use [kit](https://github.com/kinode-dao/kit).

```
git clone git@github.com:dr-frmr/crdt-crm.git
cd crdt-crm
kit b
```

If you have a node running locally, use `kit s` to install (use -p flag to select a port, default is 8080).

Sample usage (assuming two nodes named `fake1.os` and `fake2.os` have app installed):

On `fake1.os`:
```
m our@contacts:crdt-crm:mothu.eth '{"AddPeer": ["fake2.os@contacts:crdt-crm:mothu.eth", "ReadWrite"]}'
```

On `fake2.os`:
```
m our@contacts:crdt-crm:mothu.eth '{"AddPeer": ["fake1.os@contacts:crdt-crm:mothu.eth", "ReadWrite"]}'
```

Now, you can use the app on both nodes, and they will merge with one another.
```
m our@contacts:crdt-crm:mothu.eth '{"AddContact": ["Doria", {"description": "hai", "socials": {}}]}'
m our@contacts:crdt-crm:mothu.eth '{"RemoveContact": "Doria"}'
m our@contacts:crdt-crm:mothu.eth '{"EditContactSocial": ["Doria", "Telegram", "t.me/doria"]}'
```