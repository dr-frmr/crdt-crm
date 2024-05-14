const APP_POST_PATH = '/contacts:crdt-crm:mothu.eth/post';

// Fetch initial data and populate the UI
function init() {
    fetch('/our')
        .then(response => response.text())
        .then(data => {
            const our = data + '@contacts:crdt-crm:mothu.eth';
            document.getElementById('our').innerText = our;
        });

    fetch(`/contacts:crdt-crm:mothu.eth/state`)
        .then(response => response.json())
        .then(data => {
            console.log(data);
            updateContactsAndPeers(data);
            populateContactBookSelector(data.books);
            populateInvites(data.pending_invites);
            enableBookCreation();
            displaySelectedBook();
        });
}

// Populate invites
function populateInvites(invites) {
    // if invites is null or empty, return
    if (!invites || Object.keys(invites).length === 0) {
        document.getElementById('invites-container').innerHTML = '';
        return;
    }
    const invitesHtml = Object.entries(invites).map(([uuid, invite]) => {
        return `<form class="invite" onsubmit="acceptInvite('${uuid}', '${invite.from}', '${invite.name}'); return false;">
                <h2>From: ${invite.from.split('@')[0]}</h2>
                <p>Book name: ${invite.name}</p>
                <button type="submit">Accept Invite</button>
            </form>`;
    }).join('');
    document.getElementById('invites-container').innerHTML = '<h1>Invites</h1>' + invitesHtml;
}

function acceptInvite(uuid, from, name) {
    fetch(APP_POST_PATH, {
        method: 'POST',
        headers: {
            'Content-Type': 'application/json',
        },
        body: JSON.stringify({
            "AcceptInvite": uuid
        }),
    }).then(response => {
        if (response.ok) {
            // set the new book as the selected book using the 'from' and 'name' values
            let books = document.getElementById('contact-book-select').children;
            let newBookElement = Array.from(books).find(
                book => book.textContent === bookFullName(name, from)
            );
            if (newBookElement) {
                document.getElementById('contact-book-select').value = newBookElement.value;
                displaySelectedBook();
            } else {
                console.error('Newly accepted book not found in selector.');
            }
        }
    });
}

function bookFullName(name, owner) {
    return name + ' (' + owner.split('@')[0] + ')';
}

// Populate contact book selector
// If the selector is empty, instead of populating it, hide it and
// prompt the user to create a new book
function populateContactBookSelector(books) {
    const selector = document.getElementById('contact-book-select');
    const currentBookId = document.getElementById('contact-book-select').value;
    selector.innerHTML = ''; // Clear existing options
    for (const [uuid, book] of Object.entries(books)) {
        const option = new Option(bookFullName(book.name, book.owner), uuid);
        selector.add(option);
    }
    if (currentBookId) {
        selector.value = currentBookId;
    }
    selector.addEventListener('change', function () {
        displaySelectedBook();
    });
    if (selector.children.length === 0) {
        document.getElementById('contact-book-select-label').innerText = 'No Contact Books yet, create one below.';
        document.getElementById('contact-book-select').style.display = 'none';
    } else {
        document.getElementById('contact-book-select-label').innerText = 'Choose a Contact Book:';
        document.getElementById('contact-book-select').style.display = '';
    }
}

// Display only the selected book
function displaySelectedBook() {
    const selectedBookId = document.getElementById('contact-book-select').value;
    document.querySelectorAll('.contact-book').forEach(book => {
        book.style.display = book.id === `book-${selectedBookId}` ? '' : 'none';
    });
}

function enableBookCreation() {
    document.getElementById('createBookForm').addEventListener('submit', function (e) {
        e.preventDefault();
        const newBookName = document.getElementById('newBookName').value;
        fetch(APP_POST_PATH, {
            method: 'POST',
            headers: {
                'Content-Type': 'application/json',
            },
            body: JSON.stringify({
                "NewBook": newBookName
            }),
        }).then(response => {
            if (response.ok) {
                document.getElementById('createBookForm').reset(); // Clear the form after submission
                // set the new book as the selected book, where selector value is uuid
                // but we don't have uuid, so need to search for it (ewww)
                let books = document.getElementById('contact-book-select').children;
                let newBookElement = Array.from(books).find(
                    book => book.textContent ===
                        bookFullName(newBookName, document.getElementById('our').innerText)
                );
                if (newBookElement) {
                    document.getElementById('contact-book-select').value = newBookElement.value;
                    displaySelectedBook();
                } else {
                    console.error('Newly accepted book not found in selector.');
                }
            }
        });
    });
}

function updateContactsAndPeers(data) {
    document.getElementById("books").innerHTML = ''; // Clear existing books
    for (const [uuid, book] of Object.entries(data.books)) {
        const contactBookContent = document.createElement('div');
        contactBookContent.className = "contact-book";
        contactBookContent.id = `book-${uuid}`;
        contactBookContent.style.display = 'none';
        contactBookContent.innerHTML = `
                <h1>Book: ${book.name}</h1>
                <div id="contacts">
                </div>

                <h1>Peers</h1>
                <div id="peers">
                </div>

                <br>
                <br>
                <button type="button" class="deleteContactBookBtn">Delete Contact Book</button>
            `;
        document.getElementById("books").appendChild(contactBookContent);

        const container = document.querySelector(`#book-${uuid}`);

        // Populate contacts for each book
        populateContacts(container, book, uuid);

        // Populate peers for each book
        populatePeers(container, book, uuid);

        // Add event listeners for delete buttons
        enableDeleteContact(container, uuid);

        // Add event listener for deleting the entire contact book
        enableDeleteContactBook(container, uuid);

        // HTTP POST request to /post path for adding a contact
        enableAddContact(container, uuid);

        // HTTP POST request to /post path for inviting a peer
        enableInvitePeer(container, uuid);

        // Make description fields editable on click and save on enter
        enableEditDescription(container, uuid);

        // Add event listeners for socials edit and remove buttons
        enableEditSocials(container, uuid);
    }
}

function populateContacts(container, book, uuid) {
    const contactsHtml = Object.entries(book.contacts).map(([id, contact]) => {
        return `<div class="contact">
                <h2>${id}</h2>
                <p class="editableDescription" contenteditable="false" data-contact-id="${id}">${contact.description || '(no description, click to add)'}</p>
                <div class="socials">${Object.entries(contact.socials).map(([key, value]) => `
                    <span class="socialEntry">
                        <span>${key}:</span>
                        <span class="editableSocialValue" contenteditable="true" data-contact-id="${id}" data-social-key="${key}">${value}</span>
                        <button type="button" class="removeSocialBtn" data-contact-id="${id}" data-social-key="${key}">Remove</button>
                    </span>
                `).join('<br>')}</div>
                <div class="addSocialForm" data-contact-id="${id}">
                    <input type="text" placeholder="Social Media Name" class="socialKeyInput">
                    <input type="text" placeholder="Social Media Handle" class="socialValueInput">
                    <button type="button" class="submitSocialBtn">Add Social</button>
                </div>
                <button type="button" class="deleteContactBtn" data-contact-id="${id}">Delete</button>
            </div>`;
    }).join('');
    container.querySelector('#contacts').innerHTML =
        contactsHtml +
        `<div class="contact">
            <h1>Add Contact</h1>
            <form class="addContactForm">
                <label for="name-${uuid}">Name:</label>
                <input type="text" id="name-${uuid}" name="name" required>
                <br>
                <label for="desc-${uuid}">Description:</label>
                <input type="text" id="desc-${uuid}" name="desc">
                <br>
                <button type="submit">Add</button>
            </form>
        </div>`;
}

function populatePeers(container, book, uuid) {
    const peersHtml = Object.entries(book.peers).map(([address, status]) => {
        return `<div class="peer">
                <h2>${address.split('@')[0]}</h2>
                <p>Status: ${status}</p>
            </div>`;
    }).join('');
    container.querySelector('#peers').innerHTML =
        peersHtml +
        `<form class="peer invitePeerForm">
            <label for="peer-${uuid}">Add a new peer:</label>
            <input type="text" id="peer-${uuid}" name="peer" required>
            <br>
            <button type="submit">Send Invite</button>
        </form>`;
}

function enableDeleteContact(container, uuid) {
    container.querySelectorAll('.deleteContactBtn').forEach(button => {
        button.addEventListener('click', function () {
            const contactId = this.getAttribute('data-contact-id');
            fetch(APP_POST_PATH, {
                method: 'POST',
                headers: {
                    'Content-Type': 'application/json',
                },
                body: JSON.stringify({
                    "Update": [
                        uuid,
                        { "RemoveContact": contactId }]
                }),
            });
        });
    });
}

function enableDeleteContactBook(container, uuid) {
    container.querySelectorAll('.deleteContactBookBtn').forEach(button => {
        button.addEventListener('click', function () {
            fetch(APP_POST_PATH, {
                method: 'POST',
                headers: {
                    'Content-Type': 'application/json',
                },
                body: JSON.stringify({
                    "RemoveBook": uuid
                }),
            }).then(response => {
                if (response.ok) {
                    document.getElementById('contact-book-select').value = document.getElementById('contact-book-select').firstElementChild.value;
                    displaySelectedBook();
                }
            });
        });
    });
}

function enableAddContact(container, uuid) {
    container.querySelectorAll('.addContactForm').forEach(form => {
        form.addEventListener('submit', (e) => {
            e.preventDefault();
            const formData = new FormData(e.target);
            const data = {};
            for (const [key, value] of formData.entries()) {
                data[key] = value;
            }
            fetch(APP_POST_PATH, {
                method: 'POST',
                headers: {
                    'Content-Type': 'application/json',
                },
                body: JSON.stringify({
                    "Update": [
                        uuid,
                        {
                            "AddContact": [
                                formData.get('name'),
                                {
                                    ...(formData.get('desc') ? { "description": formData.get('desc') } : {}),
                                    "socials": Object.fromEntries(
                                        formData.getAll('customFieldName[]').map((fieldName, index) => [
                                            fieldName,
                                            formData.getAll('customFieldValue[]')[index]
                                        ])
                                    )
                                }
                            ]
                        }
                    ]
                }),
            }).then(response => {
                if (response.ok) {
                    e.target.reset(); // Clear the form values upon successful submit
                    container.querySelector(`#customFieldsContainer-${uuid}`).innerHTML = '';
                }
            });
        });
    });
}

function enableInvitePeer(container, uuid) {
    container.querySelectorAll('.invitePeerForm').forEach(form => {
        form.addEventListener('submit', (e) => {
            e.preventDefault();
            const formData = new FormData(e.target);
            const data = {};
            for (const [key, value] of formData.entries()) {
                data[key] = value;
            }
            let peer = formData.get('peer');
            if (!peer.endsWith('@contacts:crdt-crm:mothu.eth')) {
                peer = peer + '@contacts:crdt-crm:mothu.eth';
            }
            fetch(APP_POST_PATH, {
                method: 'POST',
                headers: {
                    'Content-Type': 'application/json',
                },
                body: JSON.stringify({
                    "CreateInvite": [
                        uuid,
                        peer,
                        "ReadWrite"
                    ]
                }),
            }).then(response => {
                if (response.ok) {
                    e.target.reset(); // Clear the form values upon successful submit
                }
            });
        });
    });
}

function enableEditDescription(container, uuid) {
    container.querySelectorAll('.editableDescription').forEach(description => {
        description.addEventListener('click', function () {
            this.contentEditable = true;
            this.focus();
        });
        description.addEventListener('keypress', function (e) {
            if (e.key === 'Enter') {
                e.preventDefault();
                this.contentEditable = false;
                const newDescription = this.innerText;
                const contactId = this.getAttribute('data-contact-id');
                fetch(APP_POST_PATH, {
                    method: 'POST',
                    headers: {
                        'Content-Type': 'application/json',
                    },
                    body: JSON.stringify({
                        "Update": [
                            uuid,
                            { "EditContactDescription": [contactId, newDescription] }]
                    }),
                });
            }
        });
    });
}

function enableEditSocials(container, uuid) {
    container.querySelectorAll('.editableSocialValue').forEach(social => {
        social.addEventListener('click', function () {
            this.contentEditable = true;
            this.focus();
        });
        social.addEventListener('keypress', function (e) {
            if (e.key === 'Enter') {
                e.preventDefault();
                this.contentEditable = false;
                const contactId = this.getAttribute('data-contact-id');
                const socialKey = this.getAttribute('data-social-key');
                const newSocialValue = this.innerText;
                fetch(APP_POST_PATH, {
                    method: 'POST',
                    headers: {
                        'Content-Type': 'application/json',
                    },
                    body: JSON.stringify({
                        "Update": [
                            uuid,
                            { "EditContactSocial": [contactId, socialKey, newSocialValue] }]
                    }),
                });
            }
        });
    });

    container.querySelectorAll('.removeSocialBtn').forEach(button => {
        button.addEventListener('click', function () {
            const contactId = this.getAttribute('data-contact-id');
            const socialKey = this.getAttribute('data-social-key');
            fetch(APP_POST_PATH, {
                method: 'POST',
                headers: {
                    'Content-Type': 'application/json',
                },
                body: JSON.stringify({
                    "Update": [
                        uuid,
                        { "RemoveContactSocial": [contactId, socialKey] }
                    ]
                }),
            });
        });
    });

    container.querySelectorAll('.submitSocialBtn').forEach(button => {
        button.addEventListener('click', function () {
            const contactId = this.closest('.addSocialForm').getAttribute('data-contact-id');
            const socialKey = this.previousElementSibling.previousElementSibling.value;
            const socialValue = this.previousElementSibling.value;
            if (socialKey && socialValue) {
                fetch(APP_POST_PATH, {
                    method: 'POST',
                    headers: {
                        'Content-Type': 'application/json',
                    },
                    body: JSON.stringify({
                        "Update": [
                            uuid,
                            { "EditContactSocial": [contactId, socialKey, socialValue] }
                        ]
                    }),
                }).then(() => {
                    this.previousElementSibling.previousElementSibling.value = ''; // Clear the social media name input
                    this.previousElementSibling.value = ''; // Clear the social media handle input
                });
            }
        });
    });
}

// Call init to start the application
init();

// Setup WebSocket connection
const wsProtocol = location.protocol === 'https:' ? 'wss:' : 'ws:';
const ws = new WebSocket(wsProtocol + "//" + location.host + "/contacts:crdt-crm:mothu.eth/updates");
ws.onmessage = event => {
    const data = JSON.parse(event.data);
    console.log(data);
    updateContactsAndPeers(data);
    populateContactBookSelector(data.books);
    populateInvites(data.pending_invites);
    displaySelectedBook();
};

