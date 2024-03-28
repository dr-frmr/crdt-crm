const app_path = 'contacts:crdt-crm:mothu.eth';

// Fetch initial data and populate the UI
function init() {
    fetch('/our')
        .then(response => response.text())
        .then(data => {
            const our = data + '@contacts:crdt-crm:mothu.eth';
            fetch(`/${app_path}/state`)
                .then(response => response.json())
                .then(data => {
                    console.log(data);
                    updateContactsAndPeers(data);
                    populateContactBookSelector(data.books);
                    enableCustomFieldAddition();
                    enableBookCreation();
                    displaySelectedBook(Object.keys(data.books)[0]);
                });
        });
}

// Populate contact book selector
function populateContactBookSelector(books) {
    const selector = document.getElementById('contactBookSelect');
    selector.innerHTML = ''; // Clear existing options
    for (const [uuid, book] of Object.entries(books)) {
        const option = new Option(book.name, uuid);
        selector.add(option);
    }
    selector.addEventListener('change', function () {
        const selectedBookId = this.value;
        displaySelectedBook(selectedBookId);
    });
}

// Display only the selected book
function displaySelectedBook(selectedBookId) {
    document.querySelectorAll('.contact-book').forEach(book => {
        book.style.display = book.id === `book-${selectedBookId}` ? '' : 'none';
    });
}

// Moved the event listener for adding custom fields inside a function to prevent multiple bindings
function enableCustomFieldAddition() {
    document.querySelectorAll('[id^="addCustomFieldBtn-"]').forEach(btn => {
        btn.addEventListener('click', function () {
            const uuid = this.id.split('-')[1];
            const container = document.getElementById(`customFieldsContainer-${uuid}`);
            const inputGroup = document.createElement('div');
            inputGroup.innerHTML = `
                <input type="text" placeholder="Field Name" name="customFieldName[]" required>
                <input type="text" placeholder="Field Value" name="customFieldValue[]" required>
                <button type="button" class="removeFieldBtn">X</button>
            `;
            container.appendChild(inputGroup);

            inputGroup.querySelector('.removeFieldBtn').addEventListener('click', function () {
                inputGroup.remove();
            });
        });
    });
}

function enableBookCreation() {
    document.getElementById('createBookForm').addEventListener('submit', function (e) {
        e.preventDefault();
        const newBookName = document.getElementById('newBookName').value;
        fetch(`/${app_path}/post`, {
            method: 'POST',
            headers: {
                'Content-Type': 'application/json',
            },
            body: JSON.stringify({
                "NewBook": newBookName
            }),
        });
        document.getElementById('createBookForm').reset(); // Clear the form after submission
        init(); // Refresh the UI to show the new book
    });
}

function updateContactsAndPeers(data) {
    document.getElementById("books").innerHTML = ''; // Clear existing books
    for (const [uuid, book] of Object.entries(data.books)) {
        const contactBookContent = `<div class="contact-book" id="book-${uuid}" style="display: none;">
                <h1>${book.name}</h1>
                <div id="contacts-${uuid}">
                </div>

                <h1>Add Contact</h1>
                <form id="addContactForm-${uuid}">
                    <input type="hidden" name="contactBookId" value="${uuid}">
                    <label for="name-${uuid}">Name:</label>
                    <input type="text" id="name-${uuid}" name="name" required>
                    <br>
                    <label for="desc-${uuid}">Description:</label>
                    <input type="text" id="desc-${uuid}" name="desc" required>
                    <br>
                    <div id="customFieldsContainer-${uuid}"></div>
                    <button type="button" id="addCustomFieldBtn-${uuid}">Add Custom Field</button>
                    <br>
                    <button type="submit">Add</button>
                </form>

                <h1>Peers</h1>
                <div id="peers-${uuid}">
                </div>
                <form id="invitePeerForm-${uuid}">
                    <input type="hidden" name="contactBookId" value="${uuid}">
                    <label for="peer-${uuid}">Add a new peer:</label>
                    <input type="text" id="peer-${uuid}" name="peer" required>
                    <br>
                    <button type="submit">Send Invite</button>
                </form>

                <br>
                <br>
                <form id="deleteContactBookForm-${uuid}">
                    <input type="hidden" name="contactBookId" value="${uuid}">
                    <button type="submit">Delete Contact Book</button>
                </form>
            </div>`;
        document.getElementById("books").innerHTML += contactBookContent;

        // Call function to enable addition of custom fields for each book
        enableCustomFieldAddition();

        // Convert contacts to a properly formatted HTML structure
        const contactsHtml = Object.entries(book.contacts).map(([id, contact]) => {
            return `<div class="contact">
                    <h2>${id}</h2>
                    <p class="editableDescription" contenteditable="false" data-contact-id="${id}">${contact.description}</p>
                    <div class="socials">${Object.entries(contact.socials).map(([key, value]) => `
                        <span class="editableSocial" contenteditable="false" data-contact-id="${id}" data-social-key="${key}">${key}: ${value}</span>
                        <button type="button" class="removeSocialBtn" data-contact-id="${id}" data-social-key="${key}">Remove</button>
                    `).join('<br>')}</div>
                    <div class="addSocialForm" data-contact-id="${id}">
                        <input type="text" placeholder="Social Media Name" class="socialKeyInput">
                        <input type="text" placeholder="Social Media Handle" class="socialValueInput">
                        <button type="button" class="submitSocialBtn">Add Social</button>
                    </div>
                    <button type="button" class="deleteContactBtn" data-contact-id="${id}">Delete</button>
                </div>`;
        }).join('');
        document.getElementById("contacts-" + uuid).innerHTML = contactsHtml;

        // Convert peers to a properly formatted HTML structure
        const peersHtml = Object.entries(book.peers).map(([address, status]) => {
            return `<div class="peer">
                    <h2>${address.split('@')[0]}</h2>
                    <p>Status: ${status}</p>
                </div>`;
        }).join('');
        document.getElementById("peers-" + uuid).innerHTML = peersHtml;

        // Add event listeners for delete buttons
        document.querySelectorAll('.deleteContactBtn').forEach(button => {
            button.addEventListener('click', function () {
                const contactId = this.getAttribute('data-contact-id');
                fetch(`/${app_path}/post`, {
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

        // Add event listener for deleting the entire contact book
        document.getElementById(`deleteContactBookForm-${uuid}`).addEventListener('submit', function (e) {
            e.preventDefault(); // Prevent form submission
            fetch(`/${app_path}/post`, {
                method: 'POST',
                headers: {
                    'Content-Type': 'application/json',
                },
                body: JSON.stringify({
                    "RemoveBook": uuid
                }),
            });
            // Refresh the UI to reflect the deletion
            init();
        });

        // Make description fields editable on click and save on enter
        document.querySelectorAll('.editableDescription').forEach(description => {
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
                    fetch(`/${app_path}/post`, {
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

        // Add event listeners for socials edit and remove buttons
        document.querySelectorAll('.editableSocial').forEach(social => {
            social.addEventListener('click', function () {
                this.contentEditable = true;
                this.focus();
            });
            social.addEventListener('keypress', function (e) {
                if (e.key === 'Enter') {
                    e.preventDefault();
                    this.contentEditable = false;
                    const newSocialValue = this.innerText.split(': ')[1];
                    const contactId = this.getAttribute('data-contact-id');
                    const socialKey = this.getAttribute('data-social-key');
                    fetch(`/${app_path}/post`, {
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

        document.querySelectorAll('.removeSocialBtn').forEach(button => {
            button.addEventListener('click', function () {
                const contactId = this.getAttribute('data-contact-id');
                const socialKey = this.getAttribute('data-social-key');
                fetch(`/${app_path}/post`, {
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

        // Add event listener for add social button within the form
        document.querySelectorAll('.submitSocialBtn').forEach(button => {
            button.addEventListener('click', function () {
                const contactId = this.closest('.addSocialForm').getAttribute('data-contact-id');
                const socialKey = this.previousElementSibling.previousElementSibling.value;
                const socialValue = this.previousElementSibling.value;
                if (socialKey && socialValue) {
                    fetch(`/${app_path}/post`, {
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
}

// HTTP POST request to /post path for adding a contact
document.querySelectorAll('[id^="addContactForm-"]').forEach(form => {
    form.addEventListener('submit', (e) => {
        e.preventDefault();
        const formData = new FormData(e.target);
        const uuid = formData.get('contactBookId'); // Correctly get the UUID from the form
        const data = {};
        for (const [key, value] of formData.entries()) {
            data[key] = value;
        }
        fetch(`/${app_path}/post`, {
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
                                "description": formData.get('desc'),
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
                document.getElementById(`customFieldsContainer-${uuid}`).innerHTML = ''; // Clear custom fields as well
            }
        });
    });
});

// HTTP POST request to /post path for inviting a peer
document.querySelectorAll('[id^="invitePeerForm-"]').forEach(form => {
    form.addEventListener('submit', (e) => {
        e.preventDefault();
        const formData = new FormData(e.target);
        const uuid = formData.get('contactBookId'); // Correctly get the UUID from the form
        const data = {};
        for (const [key, value] of formData.entries()) {
            data[key] = value;
        }
        let peer = formData.get('peer');
        if (!peer.endsWith('@contacts:crdt-crm:mothu.eth')) {
            peer = peer + '@contacts:crdt-crm:mothu.eth';
        }
        fetch(`/${app_path}/post`, {
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

// Call init to start the application
init();

// Setup WebSocket connection
let url = window.location.href;
if (url.endsWith('/')) {
    url = url.slice(0, -1);
}
url = `${url}/updates`;
url = url.replace(/^http(s)?:\/\//, 'ws$1://');
const ws = new WebSocket(url);
ws.onmessage = event => {
    const data = JSON.parse(event.data);
    console.log(data);
    updateContactsAndPeers(data);
};

