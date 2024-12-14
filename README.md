******# super-fast-smb-image-uploader

## Example Message Format

This project supports SMB and SFTP uploads using a unified message structure in JSON format. Below is an example of the message structure:
MVP: Manual upload + Worker Installation/Managing + Analytics
v2.0: Eventdriven
```json
{
  "type": "smb",                  // "smb" or "sftp"
  "recursive": true,              // Whether to include subfolders
  "root_folder": "/data/uploads", // Root folder (if recursive)
  "files": [                      // List of files and their destination details (if not recursive)
    {
      "source_path": "images/image1.jpg",
      "destination_path": "target_folder/image1.jpg"
    },
    {
      "source_path": "images/image2.png",
      "destination_path": "target_folder/image2.png"
    }
  ],
  "compression": {
    "enabled": false,  // Enable compression
    "quality": 80      // Compression quality (0-100)
  },
  "file_filters": ["jpg", "jpeg", "png"], // Allowed file formats
  "upload_strategy": "batch"             // "batch" or "single"
}******
