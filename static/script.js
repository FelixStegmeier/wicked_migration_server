let uploadedFiles = [];

pageSetup();

function pageSetup() {
    const dropArea = document.getElementById('drop-area');

    document.addEventListener('dragover', function(e) {
        e.preventDefault();
    });

    document.addEventListener('drop', function(e) {
        e.preventDefault();
    });

    ['dragenter', 'dragover'].forEach(eventName => {
        dropArea.addEventListener(eventName, highlight, false)
    });

    ['dragleave', 'drop'].forEach(eventName => {
        dropArea.addEventListener(eventName, unhighlight, false)
    });

    ['dragenter', 'dragover', 'dragleave', 'drop'].forEach(eventName => {
        dropArea.addEventListener(eventName, preventDefaults, false)
    });

    dropArea.addEventListener('drop', handleDrop, false);

    document.getElementById('file-upload').addEventListener('change', function(event) {
        let file_arr = [];
        for (let i = 0; i < event.target.files.length; i++) {
            file_arr.push(event.target.files[i]);
        }
        file_arr.forEach(addFile);
        event.target.value = "";
        setFileDividers(document.getElementById('file-container'));
    });

    document.getElementById('reset-files-button').addEventListener('click', function(event) {
        clear_uploadedFiles();
        showUserInfo("");
    });

    document.getElementById('submit-button').addEventListener('click', function(event) {
        overwriteUploadedFiles();

        let formData = new FormData();
        if (uploadedFiles.length > 0) {
            uploadedFiles.forEach(element => {
                formData.append('files[]', element);
            });

            fetch('/multipart', {
                    method: 'POST',
                    body: formData,
                })
                .then(
                    response => {
                        if (response.ok) {
                            downloadURL(response.url, "nm-migrated.tar")
                        } else {
                            response.text().then(body => showUserInfo(body)).catch(e => showUserInfo(e));
                        }
                    }
                ).catch(error => {
                    showUserInfo("Network error occurred. Please try again.");
                });

            uploadedFiles = [];
            showUserInfo("");
        } else {
            showUserInfo("Please add a file first");
        }
    });
}

function showUserInfo(text) {
    const userInfo = document.getElementById('user-info');
    userInfo.textContent = text;
}

function overwriteUploadedFiles() {
    uploadedFiles = [];
    for (let child of getFiles(document.getElementById('file-container'))) {
        let blob, newFile

        if (child.querySelector('#file-name').value.includes("xml")) {
            blob = new Blob([child.querySelector('#file-content-textarea').value], {
                type: 'text/plain'
            });
            newFile = new File([blob], child.querySelector('#file-name').value, {
                type: 'text/xml'
            });
        } else {
            blob = new Blob([child.querySelector('#file-content-textarea').value], {
                type: 'text/plain'
            });
            newFile = new File([blob], child.querySelector('#file-name').value, {
                type: 'text/plain'
            });
        }
        uploadedFiles.push(newFile);
    }
}

function autoResize(textarea) {
    textarea.style.height = "auto";
    textarea.style.height = textarea.scrollHeight + 'px';
}

function preventDefaults(e) {
    e.preventDefault()
    e.stopPropagation()
}

function highlight(e) {
    const dropArea = document.getElementById('drop-area');
    dropArea.classList.add('highlight')
}

function unhighlight(e) {
    const dropArea = document.getElementById('drop-area');
    dropArea.classList.remove('highlight')
}

function handleDrop(e) {
    e.preventDefault();
    e.stopPropagation();

    let dt = e.dataTransfer;
    let files = dt.files;
    let file_arr = Array.from(files);
    file_arr.forEach(addFile);
    setFileDividers(document.getElementById('file-container'));
}

function addFile(newFile) {
    if (!newFileAlreadyExists(newFile)) {
        uploadedFiles.push(newFile);
        createAndAdd(newFile);
    }
}

function createAndAdd(newFile) {
    const templateRef = document.getElementById("file-template");
    let node = templateRef.content.cloneNode(true);

    const fileTextArea = node.querySelector("#file-content-textarea");
    fileTextArea.style.height = fileTextArea.scrollHeight + 'px';

    node.querySelector('#remove-button').addEventListener('click', function(event) {
        let element = event.target.closest("#file");
        element.parentNode.removeChild(element);
        setFileDividers(document.getElementById('file-container'));
        showOrHideFilePlaceholder();
    });

    node.querySelector("#file-name").value = newFile.name;
    let reader = new FileReader()
    reader.onload = function(e) {
        fileTextArea.value = e.target.result;
        setTimeout(() => {
            fileTextArea.style.height = fileTextArea.scrollHeight + 'px';
        }, 0);
    }
    reader.readAsText(newFile);

    let fileContainer = document.getElementById('file-container');
    fileContainer.appendChild(node);

    showOrHideFilePlaceholder();
}

// Returns an array containing only all file elements of node
function getFiles(node) {
    return Array.from(node.children).filter((child) => child.className === "file");
}

// Removes all current dividers from node
// and then inserts divivders between all elements of node
function setFileDividers(node) {
    for (let divider of Array.from(node.children).filter((child) => child.className === "solid-divider")) {
        divider.remove();
    }
    let children = Array.from(node.children);
    children.pop();
    for (let child of children) {
        let newDivider = document.createElement('hr');
        newDivider.classList.add('solid-divider');
        node.insertBefore(newDivider, child.nextSibling);
    }
}

// If there are no files present a placeholder is shown, otherwise it gets hidden
function showOrHideFilePlaceholder() {
    document.getElementById('file-placeholder').hidden = getFiles(document.getElementById('file-container')).length != 0;
}

function newFileAlreadyExists(newFile) {
    let name = newFile.name;
    for (let child of getFiles(document.getElementById('file-container'))) {
        if (child.querySelector('#file-name').value === name) {
            return true;
        }
    }
    return false;
}

function clear_uploadedFiles() {
    uploadedFiles = [];
    document.getElementById('file-container').innerHTML = "";
    showOrHideFilePlaceholder();
}

function downloadURL(url, name) {
    let link = document.createElement("a");
    link.download = name;
    link.href = url;
    document.body.appendChild(link);
    link.click();
    document.body.removeChild(link);
    delete link;
}
