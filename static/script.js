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
        dropArea.addEventListener(eventName, highlightDropArea, false)
    });

    ['dragleave', 'drop'].forEach(eventName => {
        dropArea.addEventListener(eventName, unhighlightDropArea, false)
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
        clearFiles('file-container', 'file-placeholder');
        showUserInfo("");
    });

    document.getElementById('submit-button').addEventListener('click', function(event) {
        if(fileNamesAreValid() && alertIfFileContainsPassword()){

            let files = returnDocumentTextAsFiles();

            if (files.length <= 0) {
                showUserInfo("Please add a file first");
                return;
            }


            let formData = new FormData();
            files.forEach(element => {
                formData.append('files[]', element);
            });
            
            fetch('/web', {
                method: 'POST',
                body: formData,
            })
                .then(response => {
                    if (response.ok) {
                        return response.json();
                    } else {
                        throw new Error('Something went wrong');
                    }
                })
                .then(json => {
                    clearFiles('file-result-container', 'file-placeholder-result');
                    let parsed_json = JSON.parse(json);
                    showUserInfo(parsed_json.log)
                    parsed_json.files.forEach(file => 
                    {
                        createAndAddConfiguredFiles(file.fileName, file.fileContent);
                    }
                    )
                })
                .catch(error => {
                    showUserInfo("Network error occurred. Please try again.");
                });
            showUserInfo("");
        }
    });
}

function showUserInfo(text) {
    const userInfo = document.getElementById('user-info');
    userInfo.textContent = text;
    if (text !== "") {
        userInfo.classList.add('highlight-error');
    } else {
        userInfo.classList.remove('highlight-error');
    }
}

function returnDocumentTextAsFiles() {
    let files = [];

    for (let child of getFiles(document.getElementById('file-container'))) {
        let blob = new Blob([child.querySelector('#file-content-textarea').value], {
            type: 'text/plain'
        });
        let newFile

        if (child.querySelector('#file-name').value.includes("xml")) {
            newFile = new File([blob], child.querySelector('#file-name').value, {
                type: 'text/xml'
            });
        } else {
            newFile = new File([blob], child.querySelector('#file-name').value, {
                type: 'text/plain'
            });
        }
        files.push(newFile);
    }

    return files;
}

function autoResize(textarea) {
    textarea.style.height = "auto";
    textarea.style.height = textarea.scrollHeight + 'px';
}

function preventDefaults(e) {
    e.preventDefault()
    e.stopPropagation()
}

function highlightDropArea(e) {
    const dropArea = document.getElementById('drop-area');
    dropArea.classList.add('highlight');
}

function unhighlightDropArea(e) {
    const dropArea = document.getElementById('drop-area');
    dropArea.classList.remove('highlight');
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

//Adds the file to the dom if it isnt already
function addFile(newFile) {
    if (!newFileAlreadyExists(newFile)) {
        createAndAdd(newFile);
    }
}

//Adds the recieved file to the dom
function createAndAdd(newFile) {
    const templateRef = document.getElementById("file-template");
    let node = templateRef.content.cloneNode(true);

    const fileTextArea = node.querySelector("#file-content-textarea");
    fileTextArea.style.height = fileTextArea.scrollHeight + 'px';

    node.querySelector('#remove-button').addEventListener('click', function(event) {
        let element = event.target.closest("#file");
        element.parentNode.removeChild(element);
        setFileDividers(document.getElementById('file-container'));
        showOrHideFilePlaceholder('file-container', 'file-placeholder');
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

    showOrHideFilePlaceholder('file-container', 'file-placeholder');
}

function createAndAddConfiguredFiles(fileName, fileContent) {
    const templateRef = document.getElementById("file-template");
    let node = templateRef.content.cloneNode(true);

    const fileTextArea = node.querySelector("#file-content-textarea");
    fileTextArea.style.height = fileTextArea.scrollHeight + 'px';

    node.querySelector('#remove-button').addEventListener('click', function(event) {
        let element = event.target.closest("#file");
        element.parentNode.removeChild(element);
        setFileDividers(document.getElementById('file-result-container'));
        showOrHideFilePlaceholder('file-result-container', 'file-placeholder-result');
    });

    node.querySelector("#file-name").value = fileName;
    fileTextArea.value = fileContent;

    let fileContainer = document.getElementById('file-result-container');
    fileContainer.appendChild(node);

    showOrHideFilePlaceholder('file-result-container', 'file-placeholder-result');
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
function showOrHideFilePlaceholder(target, placeholder) {
    document.getElementById(placeholder).hidden = getFiles(document.getElementById(target)).length != 0;
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

function clearFiles(target, placeholder) {
    document.getElementById(target).innerHTML = "";
    showOrHideFilePlaceholder(target, placeholder);
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

function alertIfFileContainsPassword() {
    let passwords = []
    let regex = /PASSWORD='.+'/i;

    for (let child of getFiles(document.getElementById('file-container'))) {
        let fileText = child.querySelector('#file-content-textarea').value;
        if (checkPswd(fileText, regex)){
            passwords.push(pswdsFound(fileText, regex));
        }
    }

    if(passwords.length > 0){
    let pswd_str = passwords.join(' ');
        return confirm("You have have a password/s in your file. Consider removing it: " + pswd_str + "\n Continue anyway?");
    }
    return true;

    function checkPswd(inputText, regex) {
        return regex.test(inputText);
    }
    function pswdsFound(inputText, regex){
        return regex.exec(inputText);
    }
}

function fileNamesAreValid(){
    let invalidNames = []

    for (let child of getFiles(document.getElementById('file-container'))) {
        let filename = child.querySelector('#file-name').value;
        if(!checkFilenameValidity(filename)){
            invalidNames.push(filename);        
        }
    }
    if(invalidNames.length>0){
        alert("Invalid file names:\n" + invalidNames.join('\n') + "\nvalid name example: ifcfg-something or something.xml")
        return false;
    }
    else{
        return true;
    }
    function checkFilenameValidity(filename) {
        let regex1 = /ifcfg-.+/i;
        let regex2 = /.+\.xml/i;
    
        return regex1.test(filename) || regex2.test(filename);
    }
}